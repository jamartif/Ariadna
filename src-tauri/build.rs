//! Compilación de Tauri. En depuración, si la interfaz aún no se ha compilado, crea un
//! `ui/dist/index.html` mínimo para que `generate_context!` (que exige que `frontendDist`
//! exista) permita compilar y probar el backend sin Node. En `release` no se crea nada:
//! `beforeBuildCommand` construye la interfaz real.

fn main() {
    if std::env::var("PROFILE").as_deref() == Ok("debug") {
        let dist = std::path::Path::new("..").join("ui").join("dist");
        if !dist.exists() && std::fs::create_dir_all(&dist).is_ok() {
            let _ = std::fs::write(
                dist.join("index.html"),
                "<!doctype html><title>Ariadna</title><p>Interfaz sin compilar.</p>\n",
            );
        }
    }
    tauri_build::build();
}
