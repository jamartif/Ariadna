//! Capa fina de Tauri: `#[tauri::command]` que solo llaman a [`AppCore`], el `Prompter` con
//! diálogos nativos, la emisión de eventos y el arranque.

use std::path::PathBuf;
use std::sync::Arc;

use ariadna_git::{
    Commit, ConflictFile, DiffScope, FileDiff, GitPath, MergeFastForward, MergeResult,
    RecentRepository, Secret,
};
use async_trait::async_trait;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use crate::core::{
    default_data_dir, locate_git, validate_external_url, AiCommitOptions, AiCommitSuggestion,
    AiContextPreview, AiSettingsPatch, AiState, AmendDraft, ApiError, ApiSnapshot, AppConfig,
    AppCore, AppEvent, AppState, Closed, CommitResult, CredentialTest, Done, EventSink, HunkArg,
    OpenResult, Prompter,
};

type Cmd<T> = Result<T, ApiError>;

// --- diálogos nativos ---------------------------------------------------------------------

struct TauriPrompter {
    app: AppHandle,
}

#[async_trait]
impl Prompter for TauriPrompter {
    async fn confirm(&self, title: &str, message: &str, detail: &str) -> bool {
        let (tx, rx) = oneshot::channel();
        let text = if detail.trim().is_empty() {
            message.to_owned()
        } else {
            format!("{message}\n\n{detail}")
        };
        self.app
            .dialog()
            .message(text)
            .title(title)
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Continuar".to_owned(),
                "Cancelar".to_owned(),
            ))
            .show(move |accepted| {
                let _ = tx.send(accepted);
            });
        rx.await.unwrap_or(false)
    }

    async fn pick_folder(&self) -> Option<PathBuf> {
        let (tx, rx) = oneshot::channel();
        self.app.dialog().file().pick_folder(move |p| {
            let _ = tx.send(p);
        });
        rx.await.ok().flatten().and_then(|p| p.into_path().ok())
    }
}

// --- eventos ------------------------------------------------------------------------------

struct TauriSink {
    app: AppHandle,
}

impl EventSink for TauriSink {
    fn emit(&self, event: AppEvent) {
        let result = match &event {
            AppEvent::Snapshot(p) => self.app.emit("snapshot", p),
            AppEvent::TabUpdated(p) => self.app.emit("tab-updated", p),
            AppEvent::Session(p) => self.app.emit("session-event", p),
            AppEvent::TabsChanged(p) => self.app.emit("tabs-changed", p),
            AppEvent::CloneProgress(p) => self.app.emit("clone-progress", p),
        };
        if let Err(e) = result {
            eprintln!("no se pudo emitir el evento: {e}");
        }
    }
}

// --- comandos de aplicación ---------------------------------------------------------------

#[tauri::command]
async fn app_state(core: State<'_, AppCore>) -> Cmd<AppState> {
    Ok(core.app_state())
}

#[tauri::command]
async fn pick_folder(core: State<'_, AppCore>) -> Cmd<Option<String>> {
    Ok(core.pick_folder().await)
}

#[tauri::command]
async fn open_repository(core: State<'_, AppCore>, path: String) -> Cmd<OpenResult> {
    core.open_repository(&path).await
}

#[tauri::command]
async fn close_tab(core: State<'_, AppCore>, tab_id: String) -> Cmd<Closed> {
    core.close_tab(&tab_id)
        .await
        .map(|closed| Closed { closed })
}

#[tauri::command]
async fn activate_tab(core: State<'_, AppCore>, tab_id: String) -> Cmd<()> {
    core.activate_tab(&tab_id)
}

#[tauri::command]
async fn reorder_tabs(core: State<'_, AppCore>, tab_ids: Vec<String>) -> Cmd<()> {
    core.reorder_tabs(&tab_ids)
}

#[tauri::command]
async fn toggle_favorite(core: State<'_, AppCore>, path: String) -> Cmd<Vec<RecentRepository>> {
    core.toggle_favorite(&path)
}

#[tauri::command]
async fn forget_recent(core: State<'_, AppCore>, path: String) -> Cmd<Vec<RecentRepository>> {
    core.forget_recent(&path)
}

#[tauri::command]
async fn cancel_operation(
    core: State<'_, AppCore>,
    tab_id: String,
    operation_id: u64,
) -> Cmd<bool> {
    core.cancel_operation(&tab_id, operation_id)
}

/// Clona y abre una pestaña. `secret` es un PAT de un solo uso: no se guarda (ver `clone_ops`).
#[tauri::command]
async fn clone_repository(
    core: State<'_, AppCore>,
    url: String,
    destination: String,
    branch: Option<String>,
    username: Option<String>,
    secret: Option<Secret>,
) -> Cmd<OpenResult> {
    core.clone_repository(url, destination, branch, username, secret)
        .await
}

#[tauri::command]
async fn store_credential(
    core: State<'_, AppCore>,
    url: String,
    username: String,
    secret: Secret,
) -> Cmd<()> {
    core.store_credential(&url, &username, secret).await
}

#[tauri::command]
async fn test_credential(core: State<'_, AppCore>, url: String) -> Cmd<CredentialTest> {
    core.test_credential(&url).await
}

#[tauri::command]
async fn forget_credential(core: State<'_, AppCore>, url: String, username: String) -> Cmd<()> {
    core.forget_credential(&url, &username).await
}

/// Abre una URL en el navegador del sistema. Solo `https` y solo hosts de la lista del
/// contrato (`core::external`); cualquier otra cosa es un error `POLICY`.
#[tauri::command]
async fn open_external_url(app: AppHandle, url: String) -> Cmd<()> {
    let url = validate_external_url(&url)?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| ApiError::io(format!("no se pudo abrir el navegador: {e}")))
}

// --- comandos de IA -----------------------------------------------------------------------

#[tauri::command]
async fn ai_state(core: State<'_, AppCore>, tab_id: String) -> Cmd<AiState> {
    core.ai_state(&tab_id).await
}

#[tauri::command]
async fn ai_update_settings(core: State<'_, AppCore>, patch: AiSettingsPatch) -> Cmd<AiState> {
    core.ai_update_settings(patch).await
}

#[tauri::command]
async fn ai_preview_context(
    core: State<'_, AppCore>,
    tab_id: String,
    options: Option<AiCommitOptions>,
) -> Cmd<AiContextPreview> {
    core.ai_preview_context(&tab_id, options).await
}

#[tauri::command]
async fn ai_generate_commit_message(
    core: State<'_, AppCore>,
    tab_id: String,
    options: AiCommitOptions,
) -> Cmd<AiCommitSuggestion> {
    core.ai_generate_commit_message(&tab_id, options).await
}

#[tauri::command]
async fn ai_cancel(core: State<'_, AppCore>, tab_id: String) -> Cmd<bool> {
    core.ai_cancel(&tab_id)
}

// --- comandos de pestaña ------------------------------------------------------------------

#[tauri::command]
async fn get_snapshot(core: State<'_, AppCore>, tab_id: String) -> Cmd<ApiSnapshot> {
    core.get_snapshot(&tab_id).await
}

#[tauri::command]
async fn refresh(core: State<'_, AppCore>, tab_id: String) -> Cmd<Option<ApiSnapshot>> {
    core.refresh(&tab_id).await
}

#[tauri::command]
async fn stage(core: State<'_, AppCore>, tab_id: String, paths: Vec<GitPath>) -> Cmd<()> {
    core.stage(&tab_id, paths).await
}

#[tauri::command]
async fn unstage(core: State<'_, AppCore>, tab_id: String, paths: Vec<GitPath>) -> Cmd<()> {
    core.unstage(&tab_id, paths).await
}

#[tauri::command]
async fn stage_hunk(
    core: State<'_, AppCore>,
    tab_id: String,
    path: GitPath,
    hunk: HunkArg,
) -> Cmd<()> {
    core.stage_hunk(&tab_id, path, hunk).await
}

#[tauri::command]
async fn unstage_hunk(
    core: State<'_, AppCore>,
    tab_id: String,
    path: GitPath,
    hunk: HunkArg,
) -> Cmd<()> {
    core.unstage_hunk(&tab_id, path, hunk).await
}

#[tauri::command]
async fn discard(core: State<'_, AppCore>, tab_id: String, paths: Vec<GitPath>) -> Cmd<Done> {
    core.discard(&tab_id, paths).await
}

#[tauri::command]
async fn commit(
    core: State<'_, AppCore>,
    tab_id: String,
    message: String,
    allow_detached: bool,
) -> Cmd<CommitResult> {
    core.commit(&tab_id, message, allow_detached).await
}

#[tauri::command]
async fn amend(core: State<'_, AppCore>, tab_id: String, message: String) -> Cmd<String> {
    core.amend(&tab_id, message).await
}

#[tauri::command]
async fn amend_draft(core: State<'_, AppCore>, tab_id: String) -> Cmd<AmendDraft> {
    core.amend_draft(&tab_id).await
}

#[tauri::command]
async fn diff(
    core: State<'_, AppCore>,
    tab_id: String,
    scope: DiffScope,
    paths: Vec<GitPath>,
) -> Cmd<Vec<FileDiff>> {
    core.diff(&tab_id, scope, paths).await
}

#[tauri::command]
async fn log(
    core: State<'_, AppCore>,
    tab_id: String,
    limit: usize,
    query: Option<String>,
) -> Cmd<Vec<Commit>> {
    core.log(&tab_id, limit, query).await
}

#[tauri::command]
async fn log_page(
    core: State<'_, AppCore>,
    tab_id: String,
    skip: usize,
    limit: usize,
    query: Option<String>,
) -> Cmd<Vec<Commit>> {
    core.log_page(&tab_id, skip, limit, query).await
}

#[tauri::command]
async fn create_branch(
    core: State<'_, AppCore>,
    tab_id: String,
    name: String,
    start_point: Option<String>,
) -> Cmd<()> {
    core.create_branch(&tab_id, name, start_point).await
}

#[tauri::command]
async fn rename_branch(
    core: State<'_, AppCore>,
    tab_id: String,
    old: String,
    new: String,
) -> Cmd<()> {
    core.rename_branch(&tab_id, old, new).await
}

#[tauri::command]
async fn checkout(core: State<'_, AppCore>, tab_id: String, reference: String) -> Cmd<Done> {
    core.checkout(&tab_id, reference).await
}

#[tauri::command]
async fn delete_branch(
    core: State<'_, AppCore>,
    tab_id: String,
    name: String,
    force: bool,
) -> Cmd<Done> {
    core.delete_branch(&tab_id, name, force).await
}

#[tauri::command]
async fn merge(
    core: State<'_, AppCore>,
    tab_id: String,
    branch: String,
    fast_forward: MergeFastForward,
) -> Cmd<MergeResult> {
    core.merge(&tab_id, branch, fast_forward).await
}

/// Fusiona sin conflictos: si los hay, aborta y devuelve `MERGE_CONFLICTS`.
#[tauri::command]
async fn merge_clean(
    core: State<'_, AppCore>,
    tab_id: String,
    branch: String,
    fast_forward: MergeFastForward,
) -> Cmd<MergeResult> {
    core.merge_clean(&tab_id, branch, fast_forward).await
}

#[tauri::command]
async fn continue_integration(core: State<'_, AppCore>, tab_id: String) -> Cmd<()> {
    core.continue_integration(&tab_id).await
}

#[tauri::command]
async fn abort_integration(core: State<'_, AppCore>, tab_id: String) -> Cmd<Done> {
    core.abort_integration(&tab_id).await
}

#[tauri::command]
async fn get_conflicts(core: State<'_, AppCore>, tab_id: String) -> Cmd<Vec<ConflictFile>> {
    core.get_conflicts(&tab_id).await
}

#[tauri::command]
async fn write_conflict_resolution(
    core: State<'_, AppCore>,
    tab_id: String,
    path: GitPath,
    contents: Vec<u8>,
) -> Cmd<Done> {
    core.write_conflict_resolution(&tab_id, path, contents)
        .await
}

#[tauri::command]
async fn mark_resolved(core: State<'_, AppCore>, tab_id: String, path: GitPath) -> Cmd<()> {
    core.mark_resolved(&tab_id, path).await
}

#[tauri::command]
async fn fetch(core: State<'_, AppCore>, tab_id: String, remote: String) -> Cmd<()> {
    core.fetch(&tab_id, remote).await
}

#[tauri::command]
async fn pull(
    core: State<'_, AppCore>,
    tab_id: String,
    remote: String,
    refspec: Option<String>,
    rebase: bool,
) -> Cmd<MergeResult> {
    core.pull(&tab_id, remote, refspec, rebase).await
}

#[tauri::command]
async fn push(
    core: State<'_, AppCore>,
    tab_id: String,
    remote: String,
    refspec: Option<String>,
    set_upstream: bool,
) -> Cmd<()> {
    core.push(&tab_id, remote, refspec, set_upstream).await
}

// --- arranque -----------------------------------------------------------------------------

/// Arranca la aplicación.
pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = match default_data_dir() {
                Ok(d) => d,
                Err(_) => app.path().app_config_dir()?,
            };
            let core = AppCore::new(
                AppConfig {
                    data_dir,
                    git: locate_git(),
                    // En macOS el `recents.json` del Swift es el mismo archivo.
                    legacy_recents: None,
                },
                Arc::new(TauriPrompter {
                    app: handle.clone(),
                }),
                Arc::new(TauriSink { app: handle }),
            );
            app.manage(core.clone());
            tauri::async_runtime::spawn(async move {
                core.restore_session().await;
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Cerrar la ventana sigue el mismo camino que cerrar pestañas (spec §3.1).
                api.prevent_close();
                let app = window.app_handle().clone();
                tauri::async_runtime::spawn(async move {
                    let core = app.state::<AppCore>().inner().clone();
                    if core.shutdown().await {
                        app.exit(0);
                    }
                });
            }
        })
        .invoke_handler(tauri::generate_handler![
            app_state,
            pick_folder,
            open_repository,
            close_tab,
            activate_tab,
            reorder_tabs,
            toggle_favorite,
            forget_recent,
            cancel_operation,
            open_external_url,
            clone_repository,
            store_credential,
            test_credential,
            forget_credential,
            ai_state,
            ai_update_settings,
            ai_preview_context,
            ai_generate_commit_message,
            ai_cancel,
            get_snapshot,
            refresh,
            stage,
            unstage,
            stage_hunk,
            unstage_hunk,
            discard,
            commit,
            amend,
            amend_draft,
            diff,
            log,
            log_page,
            merge_clean,
            create_branch,
            rename_branch,
            checkout,
            delete_branch,
            merge,
            continue_integration,
            abort_integration,
            get_conflicts,
            write_conflict_resolution,
            mark_resolved,
            fetch,
            pull,
            push,
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("error al ejecutar Ariadna: {e}");
        std::process::exit(1);
    }
}
