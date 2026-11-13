//! Seguridad de rutas y referencias (spec §4.1 y §4.2).
//!
//! Todas las rutas que llegan de la UI (o de un repo hostil) pasan por aquí antes de
//! tocar git o el disco. La política es **cerrada y la misma en todos los sistemas**:
//! un repo clonado en Linux debe poder abrirse en Windows sin trampas, así que se
//! rechazan también las formas peligrosas solo en NTFS/HFS+.
//!
//! No se hace E/S: la comprobación de symlinks/junctions que escapan se hace en el
//! motor sobre rutas reales; aquí solo se valida el texto.

use std::ffi::OsString;
use std::path::Path;

use crate::models::bytes_to_os_string;

/// Motivo por el que una ruta o una referencia se rechaza.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PathSafetyError {
    #[error("ruta vacía")]
    Empty,
    #[error("la ruta contiene un byte NUL")]
    Nul,
    #[error("la ruta no es UTF-8 válido y no puede comprobarse en este sistema")]
    NotUtf8,
    #[error("ruta absoluta, con unidad o UNC: {0:?}")]
    Absolute(String),
    #[error("la ruta escapa del repositorio (`..` o `.`): {0:?}")]
    Escapes(String),
    #[error("barra invertida no permitida en rutas de repo: {0:?}")]
    Backslash(String),
    #[error("componente vacío (`//` o barra final): {0:?}")]
    EmptyComponent(String),
    #[error("componente reservado `.git` (o variante NTFS/HFS): {0:?}")]
    DotGit(String),
    #[error("nombre reservado de Windows (CON, NUL, AUX, COM1…): {0:?}")]
    ReservedName(String),
    #[error("flujo de datos alternativo o `:` en la ruta: {0:?}")]
    AlternateStream(String),
    #[error("componente que termina en punto o espacio: {0:?}")]
    TrailingDotOrSpace(String),
    #[error("el argumento empieza por `-` y se leería como opción de git: {0:?}")]
    LeadingDash(String),
    #[error("referencia no válida ({reason}): {name:?}")]
    InvalidRef { name: String, reason: &'static str },
}

fn lossy(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Valida una ruta relativa al repo (separador `/`) con la política completa:
/// [`validate_repo_path_components`] más rechazo de un primer byte `-`.
pub fn validate_repo_path(path: &[u8]) -> Result<(), PathSafetyError> {
    validate_repo_path_components(path)?;
    if path.first() == Some(&b'-') {
        return Err(PathSafetyError::LeadingDash(lossy(path)));
    }
    Ok(())
}

/// Valida una ruta del sistema (`Path`/`OsStr`) con [`validate_repo_path`].
pub fn validate_repo_os_path(path: &Path) -> Result<(), PathSafetyError> {
    validate_repo_path(&os_to_bytes(path)?)
}

#[cfg(unix)]
fn os_to_bytes(path: &Path) -> Result<Vec<u8>, PathSafetyError> {
    use std::os::unix::ffi::OsStrExt;
    Ok(path.as_os_str().as_bytes().to_vec())
}

#[cfg(not(unix))]
fn os_to_bytes(path: &Path) -> Result<Vec<u8>, PathSafetyError> {
    path.to_str()
        .map(|s| s.as_bytes().to_vec())
        .ok_or(PathSafetyError::NotUtf8)
}

/// Todas las reglas salvo la del `-` inicial (que se neutraliza con `./`, ver [`git_literal_arg`]).
pub fn validate_repo_path_components(path: &[u8]) -> Result<(), PathSafetyError> {
    if path.is_empty() {
        return Err(PathSafetyError::Empty);
    }
    if path.contains(&0) {
        return Err(PathSafetyError::Nul);
    }
    let text = || lossy(path);
    // Absolutas: `/x`, unidades `C:`, UNC `\\srv`, `\\?\C:` (estas dos caen en Backslash).
    if path[0] == b'/' {
        return Err(PathSafetyError::Absolute(text()));
    }
    if path.len() >= 2 && path[0].is_ascii_alphabetic() && path[1] == b':' {
        return Err(PathSafetyError::Absolute(text()));
    }
    if path.contains(&b'\\') {
        return Err(if path.starts_with(b"\\\\") {
            PathSafetyError::Absolute(text())
        } else {
            PathSafetyError::Backslash(text())
        });
    }
    if path.contains(&b':') {
        return Err(PathSafetyError::AlternateStream(text()));
    }
    for component in path.split(|b| *b == b'/') {
        validate_component(component, path)?;
    }
    Ok(())
}

fn validate_component(component: &[u8], whole: &[u8]) -> Result<(), PathSafetyError> {
    let text = || lossy(whole);
    if component.is_empty() {
        return Err(PathSafetyError::EmptyComponent(text()));
    }
    if component == b"." || component == b".." {
        return Err(PathSafetyError::Escapes(text()));
    }
    if is_dot_git_variant(component) {
        return Err(PathSafetyError::DotGit(text()));
    }
    if matches!(component.last(), Some(b'.' | b' ')) {
        return Err(PathSafetyError::TrailingDotOrSpace(text()));
    }
    if is_reserved_windows_name(component) {
        return Err(PathSafetyError::ReservedName(text()));
    }
    Ok(())
}

/// Caracteres que HFS+ ignora al comparar nombres (U+200C–U+200F, U+202A–U+202E,
/// U+206A–U+206F, U+FEFF). Git los trata así para detectar `.git` disfrazado.
fn is_hfs_ignorable(c: char) -> bool {
    matches!(c as u32, 0x200C..=0x200F | 0x202A..=0x202E | 0x206A..=0x206F | 0xFEFF)
}

/// `.git`, `.GIT`, `.git.`, `.git `, `git~1`, `.git::$INDEX_ALLOCATION`, `.g\u{200c}it`…
fn is_dot_git_variant(component: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(component) else {
        return false;
    };
    let cleaned: String = text.chars().filter(|c| !is_hfs_ignorable(*c)).collect();
    let lower = cleaned.to_lowercase();
    let trimmed = lower.trim_end_matches(['.', ' ']);
    if trimmed == ".git" {
        return true;
    }
    // Nombre corto 8.3 que NTFS asigna a `.git`: `git~1`, `git~2`…
    if let Some(rest) = trimmed.strip_prefix("git~") {
        if !rest.is_empty() && rest.bytes().all(|b| b.is_ascii_digit()) {
            return true;
        }
    }
    false
}

/// `CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`, `CONIN$`, `CONOUT$`, con o sin extensión
/// (`con.txt` también está reservado) y sin importar mayúsculas.
fn is_reserved_windows_name(component: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(component) else {
        return false;
    };
    let stem = text
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end_matches(' ')
        .to_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    for prefix in ["COM", "LPT"] {
        if let Some(rest) = stem.strip_prefix(prefix) {
            let mut chars = rest.chars();
            if let (Some(d), None) = (chars.next(), chars.next()) {
                // 1-9 y los superíndices ¹ ² ³, que Windows también reserva.
                if matches!(d, '1'..='9' | '\u{00B9}' | '\u{00B2}' | '\u{00B3}') {
                    return true;
                }
            }
        }
    }
    false
}

/// Convierte una ruta ya validada en un argumento que git leerá **literalmente**:
/// una ruta que empieza por `-` se antepone con `./` para que no sea una opción.
/// Debe usarse con `--` antes y `GIT_LITERAL_PATHSPECS=1` / `--literal-pathspecs`
/// (para que `:(glob)*`, `*.c` o `:/` no se interpreten como pathspec mágico).
pub fn git_literal_arg(path: &[u8]) -> Result<OsString, PathSafetyError> {
    validate_repo_path_components(path)?;
    if path[0] == b'-' {
        let mut prefixed = Vec::with_capacity(path.len() + 2);
        prefixed.extend_from_slice(b"./");
        prefixed.extend_from_slice(path);
        return Ok(bytes_to_os_string(&prefixed));
    }
    Ok(bytes_to_os_string(path))
}

/// Argumentos `["--", ruta1, ruta2…]` para pasar rutas a git de forma literal.
/// Falla en cuanto una ruta no es segura.
pub fn git_pathspec_args<I, P>(paths: I) -> Result<Vec<OsString>, PathSafetyError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<[u8]>,
{
    let mut args = vec![OsString::from("--")];
    for p in paths {
        args.push(git_literal_arg(p.as_ref())?);
    }
    Ok(args)
}

/// Cuerpo para `--pathspec-from-file=- --pathspec-file-nul`: rutas separadas por NUL
/// (con `./` ante un `-` inicial). Evita el límite de línea de comandos de Windows.
pub fn pathspec_stdin<I, P>(paths: I) -> Result<Vec<u8>, PathSafetyError>
where
    I: IntoIterator<Item = P>,
    P: AsRef<[u8]>,
{
    let mut out = Vec::new();
    for p in paths {
        let p = p.as_ref();
        validate_repo_path_components(p)?;
        if p[0] == b'-' {
            out.extend_from_slice(b"./");
        }
        out.extend_from_slice(p);
        out.push(0);
    }
    Ok(out)
}

/// Valida un nombre de rama/etiqueta/remoto con las reglas de `git check-ref-format`
/// (modo `--branch`: sin necesidad de `/`) y rechaza los que empiezan por `-`.
pub fn validate_ref(name: &str) -> Result<(), PathSafetyError> {
    let bad = |reason: &'static str| {
        Err(PathSafetyError::InvalidRef {
            name: name.to_owned(),
            reason,
        })
    };
    if name.is_empty() {
        return bad("vacía");
    }
    if name.starts_with('-') {
        return bad("empieza por `-`");
    }
    if name == "@" {
        return bad("no puede ser `@`");
    }
    if name.starts_with('/') || name.ends_with('/') {
        return bad("empieza o termina en `/`");
    }
    if name.ends_with('.') {
        return bad("termina en `.`");
    }
    if name.contains("..") {
        return bad("contiene `..`");
    }
    if name.contains("//") {
        return bad("contiene `//`");
    }
    if name.contains("@{") {
        return bad("contiene `@{`");
    }
    for c in name.chars() {
        if (c as u32) < 0x20 || c == '\u{7f}' {
            return bad("carácter de control");
        }
        if matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\') {
            return bad("carácter prohibido");
        }
    }
    for component in name.split('/') {
        if component.starts_with('.') {
            return bad("un componente empieza por `.`");
        }
        if component.ends_with(".lock") {
            return bad("un componente termina en `.lock`");
        }
    }
    Ok(())
}

/// Valida un argumento de revisión (hash, `HEAD~2`, `origin/main`, `a..b`…) que se
/// pasará a git: sin `-` inicial, NUL, espacios ni caracteres de control.
pub fn validate_revision_arg(rev: &str) -> Result<(), PathSafetyError> {
    let bad = |reason: &'static str| {
        Err(PathSafetyError::InvalidRef {
            name: rev.to_owned(),
            reason,
        })
    };
    if rev.is_empty() {
        return bad("vacía");
    }
    if rev.starts_with('-') {
        return bad("empieza por `-`");
    }
    if rev
        .chars()
        .any(|c| (c as u32) < 0x20 || c == '\u{7f}' || c == ' ')
    {
        return bad("espacio o carácter de control");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn ok(p: &str) {
        assert_eq!(
            validate_repo_path(p.as_bytes()),
            Ok(()),
            "debía aceptarse: {p:?}"
        );
    }
    fn err(p: &str) -> PathSafetyError {
        validate_repo_path(p.as_bytes()).expect_err(p)
    }

    #[test]
    fn accepts_ordinary_paths_with_spaces_and_unicode() {
        for p in [
            "src/main.rs",
            "mi archivo.txt",
            "notas/cañón archivo.txt",
            "日本語/ファイル.md",
            "*.c",
            "a/b/c.d.e",
            ".gitignore",
            ".github/workflows/ci.yml",
            "gitlab.txt",
            "console/readme",
            "comx",
            "COM10",
            "git~x",
        ] {
            ok(p);
        }
    }

    #[test]
    fn rejects_escapes_and_absolute() {
        assert!(matches!(err("../x"), PathSafetyError::Escapes(_)));
        assert!(matches!(err("a/../../x"), PathSafetyError::Escapes(_)));
        assert!(matches!(err("a/./b"), PathSafetyError::Escapes(_)));
        assert!(matches!(err("/etc/passwd"), PathSafetyError::Absolute(_)));
        assert!(matches!(err("C:/x"), PathSafetyError::Absolute(_)));
        assert!(matches!(err("c:x"), PathSafetyError::Absolute(_)));
        assert!(matches!(
            err("\\\\server\\share\\x"),
            PathSafetyError::Absolute(_)
        ));
        assert!(matches!(err("\\\\?\\C:\\x"), PathSafetyError::Absolute(_)));
        assert!(matches!(err("a\\b"), PathSafetyError::Backslash(_)));
        assert!(matches!(err("a//b"), PathSafetyError::EmptyComponent(_)));
        assert!(matches!(err("a/"), PathSafetyError::EmptyComponent(_)));
        assert_eq!(validate_repo_path(b""), Err(PathSafetyError::Empty));
        assert_eq!(validate_repo_path(b"a\0b"), Err(PathSafetyError::Nul));
    }

    #[test]
    fn rejects_dot_git_and_ntfs_variants() {
        for p in [
            ".git",
            ".git/config",
            "sub/.git/hooks/pre-commit",
            ".GIT/x",
            ".Git",
            ".git.",
            ".git ",
            ".git. .",
            "git~1",
            "GIT~1/config",
            "sub/GiT~2",
            ".git::$INDEX_ALLOCATION",
            ".g\u{200c}it/x",
            ".gi\u{feff}t",
        ] {
            let e = validate_repo_path(p.as_bytes()).expect_err(p);
            assert!(
                matches!(
                    e,
                    PathSafetyError::DotGit(_) | PathSafetyError::AlternateStream(_)
                ),
                "{p:?} -> {e:?}"
            );
        }
    }

    #[test]
    fn rejects_reserved_windows_names() {
        for p in [
            "CON",
            "con",
            "NUL",
            "aux",
            "PRN",
            "COM1",
            "com9",
            "LPT1",
            "lpt3",
            "dir/CON.txt",
            "nul.tar.gz",
            "CONIN$",
            "CONOUT$",
            "COM\u{00B9}",
            "con ",
        ] {
            let e = validate_repo_path(p.as_bytes()).expect_err(p);
            assert!(
                matches!(
                    e,
                    PathSafetyError::ReservedName(_) | PathSafetyError::TrailingDotOrSpace(_)
                ),
                "{p:?} -> {e:?}"
            );
        }
        assert!(matches!(err("CON"), PathSafetyError::ReservedName(_)));
    }

    #[test]
    fn rejects_alternate_streams_and_trailing_dot_space() {
        assert!(matches!(
            err("file.txt:stream"),
            PathSafetyError::AlternateStream(_)
        ));
        assert!(matches!(
            err("file.txt::$DATA"),
            PathSafetyError::AlternateStream(_)
        ));
        assert!(matches!(
            err(":(glob)*"),
            PathSafetyError::AlternateStream(_)
        ));
        assert!(matches!(
            err("dir./x"),
            PathSafetyError::TrailingDotOrSpace(_)
        ));
        assert!(matches!(err("x."), PathSafetyError::TrailingDotOrSpace(_)));
        assert!(matches!(
            err("dir/x "),
            PathSafetyError::TrailingDotOrSpace(_)
        ));
    }

    #[test]
    fn leading_dash_is_rejected_by_strict_validation() {
        assert!(matches!(err("-rf"), PathSafetyError::LeadingDash(_)));
        assert!(matches!(
            err("--upload-pack=evil"),
            PathSafetyError::LeadingDash(_)
        ));
        // Un guion dentro del nombre es inocuo.
        ok("a/-rf");
        ok("a-b");
    }

    #[test]
    fn literal_arg_neutralizes_dash_and_magic_pathspecs() {
        assert_eq!(git_literal_arg(b"-rf").unwrap(), OsString::from("./-rf"));
        assert_eq!(git_literal_arg(b"*.c").unwrap(), OsString::from("*.c"));
        assert_eq!(
            git_literal_arg("cañón y más.txt".as_bytes()).unwrap(),
            OsString::from("cañón y más.txt")
        );
        // `:(glob)*` jamás llega a git: lleva `:`.
        assert!(git_literal_arg(b":(glob)*").is_err());
        assert!(git_literal_arg(b"../x").is_err());
        assert!(git_literal_arg(b".git/config").is_err());
    }

    #[test]
    fn pathspec_args_put_double_dash_first() {
        let args = git_pathspec_args(["a b.txt", "-rf", "*.c"]).unwrap();
        let text: Vec<_> = args
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(text, ["--", "a b.txt", "./-rf", "*.c"]);
        assert!(git_pathspec_args(["ok", "../mal"]).is_err());
    }

    #[test]
    fn pathspec_stdin_is_nul_separated() {
        let body = pathspec_stdin(["a b", "-x", "ñ"]).unwrap();
        assert_eq!(body, "a b\0./-x\0ñ\0".as_bytes());
        assert!(pathspec_stdin(["bien", "con\0nul"]).is_err());
    }

    #[test]
    fn os_path_validation() {
        assert!(validate_repo_os_path(Path::new("src/lib.rs")).is_ok());
        assert!(validate_repo_os_path(Path::new("../fuera")).is_err());
    }

    #[test]
    fn ref_validation_matches_check_ref_format_rules() {
        for good in [
            "main",
            "feature/x",
            "release-1.0",
            "ñandú",
            "a@b",
            "v1.0.0",
            "x/y/z",
        ] {
            assert_eq!(validate_ref(good), Ok(()), "{good}");
        }
        for bad in [
            "", "-rf", "--force", "@", "/a", "a/", "a//b", "a..b", ".hidden", "a/.b", "a.lock",
            "a/b.lock", "a b", "a~1", "a^", "a:b", "a?", "a*", "a[0]", "a\\b", "a@{1}", "a.",
            "a\tb", "a\u{7f}",
        ] {
            assert!(validate_ref(bad).is_err(), "debía rechazarse: {bad:?}");
        }
    }

    #[test]
    fn revision_arg_validation() {
        for good in [
            "HEAD",
            "HEAD~2",
            "origin/main",
            "abc123",
            "a..b",
            "main^{commit}",
        ] {
            assert_eq!(validate_revision_arg(good), Ok(()));
        }
        for bad in ["", "-x", "--output=/etc/passwd", "a b", "a\nb"] {
            assert!(validate_revision_arg(bad).is_err());
        }
    }

    /// Contraste con git real: nuestras reglas no deben aceptar nada que git rechace.
    #[test]
    fn ref_validation_agrees_with_real_git() {
        let candidates = [
            "main",
            "feature/x",
            "a b",
            "a..b",
            "-x",
            "@",
            "a@{1}",
            "x.lock",
            "a/.b",
            "a//b",
            "a/",
            "ñandú",
            "a~",
            "a^",
            "a:b",
            "a?",
            "a*",
            "a[",
            "a\\b",
            "a.",
            "@x",
            "HEAD",
        ];
        for c in candidates {
            let git = std::process::Command::new("git")
                .args(["check-ref-format", "--branch", c])
                .output();
            let Ok(out) = git else {
                eprintln!("git no disponible: se omite el contraste");
                return;
            };
            let git_ok = out.status.success();
            let ours_ok = validate_ref(c).is_ok();
            // Si git lo rechaza, nosotros también. (`HEAD` lo acepta check-ref-format --branch
            // según versión; no lo exigimos.)
            // `HEAD` como nombre de rama lo rechaza `--branch` pero no check-ref-format a secas.
            if !git_ok && c != "HEAD" {
                assert!(!ours_ok, "git rechaza {c:?} pero lo aceptamos");
            }
            // `--branch` expande `@`/`HEAD` a la rama actual; como nombre nuevo no valen.
            if git_ok && c != "HEAD" && c != "@" {
                assert!(ours_ok, "git acepta {c:?} pero lo rechazamos");
            }
        }
    }
}
