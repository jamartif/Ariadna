//! Menú nativo de la aplicación como **datos** (sin dependencia de la ventana de Tauri).
//!
//! La capa Tauri (`tauri_layer.rs`) construye el menú real a partir de esta especificación y,
//! al activarse un elemento, o bien emite `menu-action { id }` a la interfaz (que lo ejecuta por
//! el **registro único de atajos**: `ui/src/lib/shortcuts/registry.ts`) o lo resuelve en Rust
//! (salir, abrir enlaces de ayuda). El acelerador de cada elemento con `shortcut` es el mismo
//! combo del registro; un test de Vitest lo comprueba contra `ui/src/lib/api/fixtures/menu.json`,
//! que genera el test de este módulo.
//!
//! macOS lleva el menú de aplicación (Acerca de, Ajustes, Salir) delante; Windows y Linux los
//! tienen en «Archivo». En Windows y Linux el menú **no se adjunta** a la ventana (la barra de
//! título es de la interfaz y una barra nativa la duplicaría): las mismas acciones están en los
//! atajos y en la paleta de comandos.

use serde::Serialize;

/// Elementos estándar del sistema (se construyen con `PredefinedMenuItem`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    About,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Minimize,
    Maximize,
    Fullscreen,
    Hide,
    HideOthers,
    ShowAll,
}

/// Qué hace un elemento propio al activarse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type", content = "value")]
pub enum Effect {
    /// Lo ejecuta la interfaz por su id del registro de atajos.
    Shortcut(&'static str),
    /// Salir por el mismo camino que cerrar la ventana (confirma el trabajo pendiente).
    Quit,
    /// Abre un enlace fijo en el navegador (pasa por `validate_external_url`).
    Url(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum Entry {
    Separator,
    Role {
        role: Role,
    },
    Item {
        id: &'static str,
        label: &'static str,
        /// Acelerador de Tauri (`CmdOrCtrl+O`). Debe coincidir con el combo del registro.
        accelerator: Option<&'static str>,
        effect: Effect,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Submenu {
    pub label: &'static str,
    pub entries: Vec<Entry>,
}

/// Repositorio y seguimiento de incidencias (hosts de la lista de `open_external_url`).
pub const DOCS_URL: &str = "https://github.com/jamartif/Ariadna#readme";
pub const ISSUES_URL: &str = "https://github.com/jamartif/Ariadna/issues";

const fn sep() -> Entry {
    Entry::Separator
}
const fn role(role: Role) -> Entry {
    Entry::Role { role }
}
const fn shortcut(id: &'static str, label: &'static str, accelerator: &'static str) -> Entry {
    Entry::Item {
        id,
        label,
        accelerator: Some(accelerator),
        effect: Effect::Shortcut(id),
    }
}

/// Menú de la aplicación. `mac` decide dónde van Ajustes y Salir.
pub fn app_menu(mac: bool) -> Vec<Submenu> {
    let mut menus = Vec::new();
    if mac {
        menus.push(Submenu {
            label: "Ariadna",
            entries: vec![
                role(Role::About),
                sep(),
                shortcut("settings.open", "Ajustes…", "CmdOrCtrl+,"),
                sep(),
                role(Role::Hide),
                role(Role::HideOthers),
                role(Role::ShowAll),
                sep(),
                Entry::Item {
                    id: "app.quit",
                    label: "Salir de Ariadna",
                    accelerator: Some("CmdOrCtrl+Q"),
                    effect: Effect::Quit,
                },
            ],
        });
    }
    let mut file = vec![
        shortcut("repo.open", "Abrir repositorio…", "CmdOrCtrl+O"),
        shortcut("clone.open", "Clonar repositorio…", "CmdOrCtrl+Shift+O"),
        sep(),
        shortcut("tab.new", "Nueva pestaña", "CmdOrCtrl+T"),
        shortcut("tab.close", "Cerrar pestaña", "CmdOrCtrl+W"),
    ];
    if !mac {
        file.push(sep());
        file.push(shortcut("settings.open", "Ajustes…", "CmdOrCtrl+,"));
        file.push(sep());
        file.push(Entry::Item {
            id: "app.quit",
            label: "Salir",
            accelerator: None,
            effect: Effect::Quit,
        });
    }
    menus.push(Submenu {
        label: "Archivo",
        entries: file,
    });
    menus.push(Submenu {
        label: "Editar",
        entries: vec![
            role(Role::Undo),
            role(Role::Redo),
            sep(),
            role(Role::Cut),
            role(Role::Copy),
            role(Role::Paste),
            role(Role::SelectAll),
        ],
    });
    menus.push(Submenu {
        label: "Ver",
        entries: vec![
            shortcut("changes.refresh", "Refrescar", "CmdOrCtrl+R"),
            shortcut("palette.open", "Paleta de comandos…", "CmdOrCtrl+K"),
            sep(),
            role(Role::Fullscreen),
        ],
    });
    // Mover pestañas (Alt+Mayús+←/→) no va al menú: en macOS un acelerador de menú le quitaría
    // a los campos de texto «seleccionar por palabras».
    menus.push(Submenu {
        label: "Ventana",
        entries: vec![
            role(Role::Minimize),
            role(Role::Maximize),
            sep(),
            shortcut("tab.next", "Pestaña siguiente", "Ctrl+Tab"),
            shortcut("tab.prev", "Pestaña anterior", "Ctrl+Shift+Tab"),
        ],
    });
    menus.push(Submenu {
        label: "Ayuda",
        entries: vec![
            Entry::Item {
                id: "help.docs",
                label: "Documentación de Ariadna",
                accelerator: None,
                effect: Effect::Url(DOCS_URL),
            },
            Entry::Item {
                id: "help.issues",
                label: "Notificar un problema",
                accelerator: None,
                effect: Effect::Url(ISSUES_URL),
            },
        ],
    });
    menus
}

/// Efecto de un elemento propio por id, en cualquiera de las dos variantes del menú.
pub fn effect_of(id: &str) -> Option<Effect> {
    [true, false]
        .into_iter()
        .flat_map(app_menu)
        .flat_map(|m| m.entries)
        .find_map(|e| match e {
            Entry::Item { id: i, effect, .. } if i == id => Some(effect),
            _ => None,
        })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use std::collections::HashSet;
    use std::path::PathBuf;

    use super::*;
    use crate::core::validate_external_url;

    fn items(mac: bool) -> Vec<(&'static str, Option<&'static str>, Effect)> {
        app_menu(mac)
            .into_iter()
            .flat_map(|m| m.entries)
            .filter_map(|e| match e {
                Entry::Item {
                    id,
                    accelerator,
                    effect,
                    ..
                } => Some((id, accelerator, effect)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn menus_minimos_en_ambas_variantes() {
        for mac in [true, false] {
            let labels: Vec<_> = app_menu(mac).iter().map(|m| m.label).collect();
            let expected: &[&str] = if mac {
                &["Ariadna", "Archivo", "Editar", "Ver", "Ventana", "Ayuda"]
            } else {
                &["Archivo", "Editar", "Ver", "Ventana", "Ayuda"]
            };
            assert_eq!(labels, expected, "mac={mac}");
        }
    }

    #[test]
    fn ids_y_aceleradores_sin_repetir_y_validos() {
        for mac in [true, false] {
            let all = items(mac);
            let ids: HashSet<_> = all.iter().map(|i| i.0).collect();
            assert_eq!(ids.len(), all.len(), "ids repetidos (mac={mac})");
            let accs: Vec<_> = all.iter().filter_map(|i| i.1).collect();
            let uniq: HashSet<_> = accs.iter().collect();
            assert_eq!(uniq.len(), accs.len(), "aceleradores repetidos (mac={mac})");
            for a in accs {
                a.parse::<muda::accelerator::Accelerator>()
                    .unwrap_or_else(|e| panic!("acelerador no válido {a}: {e}"));
            }
        }
    }

    #[test]
    fn open_y_refresh_tienen_su_acelerador() {
        let all = items(false);
        let acc = |id: &str| all.iter().find(|i| i.0 == id).and_then(|i| i.1);
        assert_eq!(acc("repo.open"), Some("CmdOrCtrl+O"));
        assert_eq!(acc("changes.refresh"), Some("CmdOrCtrl+R"));
    }

    #[test]
    fn efectos_de_ayuda_pasan_la_politica_de_enlaces() {
        for url in [DOCS_URL, ISSUES_URL] {
            validate_external_url(url).unwrap();
        }
        assert_eq!(effect_of("help.docs"), Some(Effect::Url(DOCS_URL)));
        assert_eq!(effect_of("app.quit"), Some(Effect::Quit));
        assert_eq!(effect_of("repo.open"), Some(Effect::Shortcut("repo.open")));
        assert_eq!(effect_of("no.existe"), None);
    }

    #[test]
    fn quit_solo_lleva_acelerador_en_mac() {
        let q = |mac| {
            items(mac)
                .into_iter()
                .find(|i| i.0 == "app.quit")
                .unwrap()
                .1
        };
        assert_eq!(q(true), Some("CmdOrCtrl+Q"));
        assert_eq!(q(false), None);
    }

    /// Fixture que lee Vitest para comprobar que el menú y el registro de atajos coinciden.
    /// `ARIADNA_UPDATE_FIXTURES=1 cargo test -p ariadna-app menu` lo regenera.
    #[test]
    fn fixture_menu_json_al_dia() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("ui/src/lib/api/fixtures/menu.json");
        let value = serde_json::json!({ "mac": app_menu(true), "other": app_menu(false) });
        let mut text = serde_json::to_string_pretty(&value).unwrap();
        text.push('\n');
        if std::env::var_os("ARIADNA_UPDATE_FIXTURES").is_some() {
            std::fs::write(&path, &text).unwrap();
        }
        let current = std::fs::read_to_string(&path)
            .expect("falta menu.json: ARIADNA_UPDATE_FIXTURES=1 cargo test -p ariadna-app menu");
        assert_eq!(
            current.replace("\r\n", "\n"),
            text,
            "menu.json desactualizado: regenéralo con ARIADNA_UPDATE_FIXTURES=1"
        );
    }
}
