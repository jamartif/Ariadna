//! Configuración de ventana y capabilities: tamaño mínimo 980x640, barra de título propia en
//! Windows y Linux, superposición en macOS y ningún permiso del plugin `opener` para la UI.

use std::path::PathBuf;

use serde_json::Value;

fn read(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(name);
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{name}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn window(name: &str) -> Value {
    read(name)["app"]["windows"][0].clone()
}

#[test]
fn tamano_minimo_980_por_640_en_todas_las_configuraciones() {
    for name in [
        "tauri.conf.json",
        "tauri.windows.conf.json",
        "tauri.linux.conf.json",
        "tauri.macos.conf.json",
    ] {
        let w = window(name);
        assert_eq!(w["minWidth"], 980, "{name}");
        assert_eq!(w["minHeight"], 640, "{name}");
        assert_eq!(w["resizable"], true, "{name}");
        assert!(w["width"].as_u64().unwrap() >= 980, "{name}");
        assert!(w["height"].as_u64().unwrap() >= 640, "{name}");
    }
}

#[test]
fn barra_de_titulo_propia_en_windows_y_linux() {
    for name in ["tauri.windows.conf.json", "tauri.linux.conf.json"] {
        let w = window(name);
        assert_eq!(w["decorations"], false, "{name}");
        assert!(w.get("titleBarStyle").is_none(), "{name}");
    }
}

#[test]
fn macos_usa_overlay_con_los_semaforos_alineados_con_la_barra() {
    let w = window("tauri.macos.conf.json");
    assert_eq!(w["titleBarStyle"], "Overlay");
    assert_eq!(w["hiddenTitle"], true);
    assert_ne!(
        w["decorations"], false,
        "los semáforos nativos deben existir"
    );
    // La barra de la UI mide 40 px (`--titlebar-h`): el botón (~14 px) queda centrado.
    let pos = &w["trafficLightPosition"];
    assert_eq!(pos["x"], 16);
    assert_eq!(pos["y"], 13);
}

#[test]
fn la_ui_no_puede_llamar_al_plugin_opener() {
    let caps = read("capabilities/default.json");
    let perms = caps["permissions"].as_array().unwrap();
    for p in perms {
        let id = p.as_str().or_else(|| p["identifier"].as_str()).unwrap();
        assert!(
            !id.starts_with("opener:"),
            "permiso de opener en la capability: {id}"
        );
        assert!(
            !id.starts_with("shell:") && !id.starts_with("fs:") && !id.starts_with("http:"),
            "permiso demasiado amplio: {id}"
        );
    }
    assert_eq!(caps["windows"], serde_json::json!(["main"]));
}
