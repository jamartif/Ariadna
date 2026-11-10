//! Capa fina de Tauri: `#[tauri::command]` que solo llaman a [`AppCore`], el `Prompter` con
//! diálogos nativos, la emisión de eventos y el arranque.

use std::path::PathBuf;
use std::sync::Arc;

use ariadna_git::{
    Commit, ConflictFile, DiffScope, FileDiff, GitPath, MergeFastForward, MergeResult,
    RecentRepository, Secret,
};
use async_trait::async_trait;
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;
use tokio::sync::oneshot;

use crate::core::menu::{app_menu, effect_of, Effect, Entry, Role};
use crate::core::{
    default_data_dir, locate_git, validate_external_url, AiCommitOptions, AiCommitSuggestion,
    AiContextPreview, AiSettingsPatch, AiState, AmendDraft, ApiError, ApiSnapshot, AppConfig,
    AppCore, AppEvent, AppState, CliStatus, Closed, CommitResult, ConflictResolution,
    ContinueResult, CredentialTest, Done, EventSink, GitDetect, GitIdentity, HunkArg,
    MergeStartResult, MergeState, OpenResult, Prompter, RemoteView, Settings, SettingsView,
    WebTarget,
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
        // Diagnóstico: `ARIADNA_TRACE_EVENTS=1` vuelca a stderr los eventos de estado.
        if std::env::var_os("ARIADNA_TRACE_EVENTS").is_some() {
            match &event {
                AppEvent::Snapshot(p) => eprintln!(
                    "[evento] snapshot tab={} revision={} cambios={}",
                    p.tab_id,
                    p.snapshot.0.revision,
                    p.snapshot.0.status.entries.len()
                ),
                AppEvent::Notice(p) => eprintln!("[evento] notice {}: {}", p.kind, p.message),
                _ => {}
            }
        }
        let result = match &event {
            AppEvent::Snapshot(p) => self.app.emit("snapshot", p),
            AppEvent::TabUpdated(p) => self.app.emit("tab-updated", p),
            AppEvent::Session(p) => self.app.emit("session-event", p),
            AppEvent::TabsChanged(p) => self.app.emit("tabs-changed", p),
            AppEvent::CloneProgress(p) => self.app.emit("clone-progress", p),
            AppEvent::Notice(p) => self.app.emit("notice", p),
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

/// Abre en el navegador una página web de un remoto del repo (repo, rama o nueva pull request).
/// La URL la construye Rust a partir de la configuración real del remoto y se valida (solo
/// `https`, sin credenciales y con el host del propio remoto); la UI solo envía el nombre del
/// remoto y la rama. Es solo un enlace: la integración por API es M8.
#[tauri::command]
async fn open_remote_url(
    app: AppHandle,
    core: State<'_, AppCore>,
    tab_id: String,
    remote: String,
    kind: WebTarget,
    branch: Option<String>,
) -> Cmd<()> {
    let url = core
        .remote_web_url(&tab_id, &remote, kind, branch.as_deref())
        .await?;
    app.opener()
        .open_url(url.as_str(), None::<&str>)
        .map_err(|e| ApiError::io(format!("no se pudo abrir el navegador: {e}")))
}

#[tauri::command]
async fn list_remotes(core: State<'_, AppCore>, tab_id: String) -> Cmd<Vec<RemoteView>> {
    core.list_remote_views(&tab_id).await
}

#[tauri::command]
async fn add_remote(
    core: State<'_, AppCore>,
    tab_id: String,
    name: String,
    url: String,
) -> Cmd<Done> {
    core.add_remote(&tab_id, name, url).await
}

#[tauri::command]
async fn rename_remote(
    core: State<'_, AppCore>,
    tab_id: String,
    old: String,
    new: String,
) -> Cmd<Done> {
    core.rename_remote(&tab_id, old, new).await
}

#[tauri::command]
async fn remove_remote(core: State<'_, AppCore>, tab_id: String, name: String) -> Cmd<Done> {
    core.remove_remote(&tab_id, name).await
}

// --- menú nativo ----------------------------------------------------------------------------

/// Carga de `menu-action`: id del atajo en el registro de la interfaz.
#[derive(Clone, Serialize)]
struct MenuActionEvent {
    id: &'static str,
}

fn role_item(app: &AppHandle, role: Role) -> tauri::Result<PredefinedMenuItem<tauri::Wry>> {
    match role {
        Role::About => PredefinedMenuItem::about(app, None, None),
        Role::Undo => PredefinedMenuItem::undo(app, None),
        Role::Redo => PredefinedMenuItem::redo(app, None),
        Role::Cut => PredefinedMenuItem::cut(app, None),
        Role::Copy => PredefinedMenuItem::copy(app, None),
        Role::Paste => PredefinedMenuItem::paste(app, None),
        Role::SelectAll => PredefinedMenuItem::select_all(app, None),
        Role::Minimize => PredefinedMenuItem::minimize(app, None),
        Role::Maximize => PredefinedMenuItem::maximize(app, None),
        Role::Fullscreen => PredefinedMenuItem::fullscreen(app, None),
        Role::Hide => PredefinedMenuItem::hide(app, None),
        Role::HideOthers => PredefinedMenuItem::hide_others(app, None),
        Role::ShowAll => PredefinedMenuItem::show_all(app, None),
    }
}

/// Construye el menú nativo a partir de la especificación de `core::menu`.
fn build_menu(app: &AppHandle, mac: bool) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    for sub in app_menu(mac) {
        let submenu = Submenu::new(app, sub.label, true)?;
        for entry in sub.entries {
            match entry {
                Entry::Separator => submenu.append(&PredefinedMenuItem::separator(app)?)?,
                Entry::Role { role } => submenu.append(&role_item(app, role)?)?,
                Entry::Item {
                    id,
                    label,
                    accelerator,
                    ..
                } => submenu.append(&MenuItem::with_id(app, id, label, true, accelerator)?)?,
            }
        }
        menu.append(&submenu)?;
    }
    Ok(menu)
}

/// Salir por el mismo camino que cerrar la ventana: confirma el trabajo pendiente (spec §3.1).
fn request_quit(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let core = app.state::<AppCore>().inner().clone();
        if core.shutdown().await {
            app.exit(0);
        }
    });
}

fn on_menu_event(app: &AppHandle, id: &str) {
    match effect_of(id) {
        Some(Effect::Shortcut(shortcut)) => {
            if let Err(e) = app.emit("menu-action", MenuActionEvent { id: shortcut }) {
                eprintln!("no se pudo emitir menu-action: {e}");
            }
        }
        Some(Effect::Quit) => request_quit(app.clone()),
        Some(Effect::Url(raw)) => {
            let opened = validate_external_url(raw).and_then(|url| {
                app.opener()
                    .open_url(url.as_str(), None::<&str>)
                    .map_err(|e| ApiError::io(format!("no se pudo abrir el navegador: {e}")))
            });
            if let Err(e) = opened {
                eprintln!("{}", e.message);
            }
        }
        None => {}
    }
}

// --- comandos de IA -----------------------------------------------------------------------

// --- ajustes (specs/rust/ipc.md «Ajustes») ---------------------------------------------------

#[tauri::command]
async fn get_settings(core: State<'_, AppCore>) -> Cmd<SettingsView> {
    Ok(core.get_settings())
}

#[tauri::command]
async fn save_settings(core: State<'_, AppCore>, settings: Settings) -> Cmd<SettingsView> {
    core.save_settings(settings).await
}

#[tauri::command]
async fn detect_git(core: State<'_, AppCore>, path: Option<String>) -> Cmd<GitDetect> {
    Ok(core.detect_git(path).await)
}

#[tauri::command]
async fn git_identity(core: State<'_, AppCore>) -> Cmd<GitIdentity> {
    Ok(core.git_identity().await)
}

#[tauri::command]
async fn get_cli_status(core: State<'_, AppCore>) -> Cmd<Vec<CliStatus>> {
    Ok(core.get_cli_status().await)
}

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

/// Empieza un merge y, si hay conflictos, lo deja en curso para resolverlos.
#[tauri::command]
async fn merge_start(
    core: State<'_, AppCore>,
    tab_id: String,
    branch: String,
    fast_forward: MergeFastForward,
) -> Cmd<MergeStartResult> {
    core.merge_start(&tab_id, branch, fast_forward).await
}

/// Estado de la integración en curso y resumen de sus conflictos.
#[tauri::command]
async fn merge_state(core: State<'_, AppCore>, tab_id: String) -> Cmd<MergeState> {
    core.merge_state(&tab_id).await
}

/// Resuelve un archivo (escribe o borra + `git add`) tras confirmación nativa.
#[tauri::command]
async fn resolve_conflict(
    core: State<'_, AppCore>,
    tab_id: String,
    path: GitPath,
    resolution: ConflictResolution,
) -> Cmd<Done> {
    core.resolve_conflict(&tab_id, path, resolution).await
}

/// Termina el merge (o continúa rebase/cherry-pick) con mensaje opcional.
#[tauri::command]
async fn merge_continue(
    core: State<'_, AppCore>,
    tab_id: String,
    message: Option<String>,
) -> Cmd<ContinueResult> {
    core.merge_continue(&tab_id, message).await
}

/// Aborta el merge en curso (confirmación nativa).
#[tauri::command]
async fn merge_abort(core: State<'_, AppCore>, tab_id: String) -> Cmd<Done> {
    core.merge_abort(&tab_id).await
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
    rebase: Option<bool>,
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
            let handle_for_menu = handle.clone();
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
            // El menú nativo solo se adjunta en macOS (barra de menús del sistema). En Windows y
            // Linux la barra de título es de la interfaz y una barra nativa la duplicaría.
            if cfg!(target_os = "macos") {
                app.set_menu(build_menu(&handle_for_menu, true)?)?;
            }
            app.manage(core.clone());
            tauri::async_runtime::spawn(async move {
                core.restore_session().await;
            });
            Ok(())
        })
        .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(focused) = event {
                // Al volver a la ventana se refresca la pestaña activa (y el sondeo de
                // respaldo solo corre con la ventana enfocada).
                let core = window.app_handle().state::<AppCore>().inner().clone();
                let focused = *focused;
                tauri::async_runtime::spawn(async move {
                    core.set_window_focus(focused);
                });
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Cerrar la ventana sigue el mismo camino que cerrar pestañas (spec §3.1).
                api.prevent_close();
                request_quit(window.app_handle().clone());
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
            open_remote_url,
            list_remotes,
            add_remote,
            rename_remote,
            remove_remote,
            clone_repository,
            store_credential,
            test_credential,
            forget_credential,
            get_settings,
            save_settings,
            detect_git,
            git_identity,
            get_cli_status,
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
            merge_start,
            merge_state,
            resolve_conflict,
            merge_continue,
            merge_abort,
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
