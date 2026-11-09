//! Mapeo de TODOS los `GitError` a `ApiError` con los códigos exactos de `ipc.md`.

use std::collections::BTreeSet;

use ariadna_git::GitError;

use crate::core::error::{ApiError, AI_CODES, ALL_CODES};

/// Una instancia de cada variante de `GitError` y el código que debe producir.
pub fn every_git_error() -> Vec<(GitError, &'static str)> {
    vec![
        (
            GitError::NotARepository { path: "/x".into() },
            "NOT_A_REPOSITORY",
        ),
        (GitError::GitNotFound("git".into()), "GIT_NOT_FOUND"),
        (
            GitError::CommandFailed {
                command: "git status".into(),
                exit_code: 128,
                stderr: "fatal: algo".into(),
            },
            "COMMAND_FAILED",
        ),
        (GitError::NothingToCommit, "NOTHING_TO_COMMIT"),
        (GitError::EmptyCommitMessage, "EMPTY_COMMIT_MESSAGE"),
        (GitError::DetachedHeadCommit, "DETACHED_HEAD_COMMIT"),
        (
            GitError::CannotAmend {
                reason: "ya publicado".into(),
            },
            "CANNOT_AMEND",
        ),
        (
            GitError::LocalChangesBlockCheckout {
                detail: "a.txt".into(),
            },
            "LOCAL_CHANGES_BLOCK_CHECKOUT",
        ),
        (
            GitError::DivergedLocalBranch {
                local: "feature".into(),
                remote: "origin/feature".into(),
            },
            "DIVERGED_LOCAL_BRANCH",
        ),
        (
            GitError::InvalidReference("-rf".into()),
            "INVALID_REFERENCE",
        ),
        (
            GitError::HunkNotFound {
                path: "a.txt".into(),
            },
            "HUNK_NOT_FOUND",
        ),
        (GitError::NoIntegrationInProgress, "NO_INTEGRATION"),
        (
            GitError::ConfirmationRequired {
                action: "descartar".into(),
            },
            "CONFIRMATION_REQUIRED",
        ),
        (
            GitError::PathEscapesRepository {
                path: "../fuera".into(),
            },
            "PATH_ESCAPES_REPOSITORY",
        ),
        (
            GitError::Timeout {
                command: "git fetch".into(),
            },
            "TIMEOUT",
        ),
        (GitError::Cancelled, "CANCELLED"),
        (GitError::Parse("raro".into()), "PARSE"),
        (
            GitError::Locked {
                lock: ".git/index.lock".into(),
            },
            "LOCKED",
        ),
        (
            GitError::DubiousOwnership {
                path: "/otro".into(),
            },
            "DUBIOUS_OWNERSHIP",
        ),
        (
            GitError::Auth {
                host: "dev.azure.com".into(),
                hint: "El token ha caducado.".into(),
            },
            "AUTH",
        ),
        (GitError::Io("disco lleno".into()), "IO"),
    ]
}

#[test]
fn cada_git_error_tiene_su_codigo_exacto_y_mensaje_en_espanol() {
    for (err, code) in every_git_error() {
        let api = ApiError::from(err.clone());
        assert_eq!(api.code, code, "{err:?}");
        assert_eq!(api.message, err.to_string(), "{err:?}");
        assert!(!api.message.is_empty());
    }
}

#[test]
fn los_codigos_cubren_exactamente_el_contrato() {
    let mapped: BTreeSet<&str> = every_git_error().iter().map(|(_, c)| *c).collect();
    assert_eq!(mapped.len(), 21, "21 variantes, 21 códigos distintos");
    let mut expected: BTreeSet<&str> = ALL_CODES.iter().copied().collect();
    // No vienen de git: REPO_MISSING (pestaña ausente), los AI_* y POLICY/RATE_LIMITED.
    assert!(expected.remove("REPO_MISSING"));
    // `merge_clean` lo produce el núcleo al abortar un merge con conflictos.
    assert!(expected.remove("MERGE_CONFLICTS"));
    // Los del estudio de conflictos (`resolve_conflict`, `merge_continue`) también son del núcleo.
    for c in [
        "CONFLICTS_PENDING",
        "CONFLICT_MARKERS_REMAIN",
        "CONFLICT_NOT_TEXT",
        "CONFLICT_NOT_FOUND",
    ] {
        assert!(expected.remove(c), "{c}");
    }
    for c in AI_CODES.iter().chain(["POLICY", "RATE_LIMITED"].iter()) {
        assert!(expected.remove(c), "{c}");
    }
    assert_eq!(mapped, expected);
    assert_eq!(ALL_CODES.len(), 39);
}

#[test]
fn el_detalle_lleva_el_dato_util_y_nunca_va_vacio() {
    let d = |e: GitError| ApiError::from(e).detail;
    assert_eq!(
        d(GitError::CommandFailed {
            command: "git x".into(),
            exit_code: 1,
            stderr: " fatal: boom \n".into()
        })
        .as_deref(),
        Some("fatal: boom")
    );
    assert_eq!(
        d(GitError::DivergedLocalBranch {
            local: "a".into(),
            remote: "origin/a".into()
        })
        .as_deref(),
        Some("origin/a")
    );
    assert_eq!(d(GitError::Cancelled), None);
    assert_eq!(
        d(GitError::LocalChangesBlockCheckout {
            detail: "  ".into()
        }),
        None
    );
}

#[test]
fn la_forma_json_es_code_message_detail() {
    let e = ApiError::from(GitError::NotARepository { path: "/x".into() });
    let v = serde_json::to_value(&e).unwrap();
    assert_eq!(v["code"], "NOT_A_REPOSITORY");
    assert_eq!(v["detail"], "/x");
    assert!(v["message"].as_str().unwrap().contains("/x"));
    // Sin detalle, la clave se omite (`detail?`).
    let v = serde_json::to_value(ApiError::from(GitError::Cancelled)).unwrap();
    assert!(v.get("detail").is_none());
    let back: ApiError = serde_json::from_value(v).unwrap();
    assert_eq!(back.code, "CANCELLED");
}

#[test]
fn errores_propios_de_la_app() {
    let e = ApiError::unknown_tab("tab-9");
    assert_eq!(
        (e.code.as_str(), e.detail.as_deref()),
        ("REPO_MISSING", Some("tab-9"))
    );
    let e = ApiError::repo_missing("/x/y");
    assert_eq!(e.code, "REPO_MISSING");
    assert!(e.message.contains("/x/y"));
    assert_eq!(ApiError::io("boom").code, "IO");
}
