//! Recientes y favoritos (G-15), compatibles con el `recents.json` de la versión Swift.
//!
//! # Formato (verificado en `RecentRepositoryStore.swift`)
//!
//! Un array JSON de objetos con las claves `path`, `name`, `lastOpened` (ISO 8601 UTC con
//! segundos, p. ej. `2023-11-14T22:13:20Z`), `isFavorite` y `lastBranch` (se omite si es nulo).
//! El Swift lo escribe con claves ordenadas e indentación; aquí se escribe igual de ordenado.
//!
//! # Ubicación
//!
//! * macOS: `~/Library/Application Support/Ariadna/recents.json` (la misma que el Swift).
//! * Windows: `%APPDATA%\Ariadna\recents.json`.
//! * Linux: `~/.config/ariadna/recents.json` (como el Swift en Linux).
//!
//! # Robustez
//!
//! La escritura es atómica (archivo temporal en la misma carpeta + `rename`) y antes de
//! sustituir el archivo se guarda una copia `recents.json.bak` del contenido válido anterior.
//! Si al leer el JSON está corrupto se recupera la copia; si ambos fallan, el archivo corrupto
//! se aparta como `recents.json.corrupt` y se empieza con una lista vacía. Nunca se pierde
//! silenciosamente un archivo ilegible.
//!
//! El almacén no bloquea el archivo: lo usa un único escritor (el gestor de sesiones).

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::models::RecentRepository;

/// Máximo de recientes no favoritos que se conservan.
pub const MAX_RECENTS: usize = 30;

/// Errores del almacén.
#[derive(Debug, thiserror::Error)]
pub enum RecentsError {
    #[error("error de E/S en {path:?}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("no se pudo serializar la lista de recientes: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("no se pudo determinar la carpeta de configuración del usuario")]
    NoConfigDir,
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> RecentsError + '_ {
    move |source| RecentsError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Cómo se obtuvo la lista al cargar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadOutcome {
    /// El archivo no existía.
    Missing,
    /// Leído sin problemas.
    Ok,
    /// El principal estaba corrupto y se recuperó la copia de respaldo.
    RecoveredFromBackup,
    /// Principal y copia ilegibles: se apartó el corrupto y se empieza vacío.
    ResetCorrupt,
}

/// Almacén de recientes en un archivo JSON.
#[derive(Debug, Clone)]
pub struct RecentStore {
    file: PathBuf,
}

impl RecentStore {
    /// Almacén en un archivo concreto.
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self { file: file.into() }
    }

    /// Almacén en `<base>/recents.json` (para tests y para instalaciones portables).
    pub fn with_base_dir(base: impl AsRef<Path>) -> Self {
        Self::new(base.as_ref().join("recents.json"))
    }

    /// Almacén en la ubicación estándar del usuario (ver documentación del módulo).
    pub fn default_location() -> Result<Self, RecentsError> {
        Ok(Self::with_base_dir(config_dir()?))
    }

    /// Ruta del archivo principal.
    pub fn file(&self) -> &Path {
        &self.file
    }

    fn backup_file(&self) -> PathBuf {
        sibling(&self.file, ".bak")
    }

    fn tmp_file(&self) -> PathBuf {
        sibling(&self.file, ".tmp")
    }

    fn corrupt_file(&self) -> PathBuf {
        sibling(&self.file, ".corrupt")
    }

    /// Carga la lista, recuperándose de archivos corruptos. Devuelve también cómo se obtuvo.
    pub fn load_with_outcome(&self) -> Result<(Vec<RecentRepository>, LoadOutcome), RecentsError> {
        match read_list(&self.file)? {
            ReadResult::Missing => {
                // Sin principal pero con copia (p. ej. borrado a mano): se usa la copia.
                if let ReadResult::Valid(items) = read_list(&self.backup_file())? {
                    return Ok((items, LoadOutcome::RecoveredFromBackup));
                }
                Ok((Vec::new(), LoadOutcome::Missing))
            }
            ReadResult::Valid(items) => Ok((items, LoadOutcome::Ok)),
            ReadResult::Corrupt => {
                let backup = read_list(&self.backup_file())?;
                // Se aparta el corrupto para poder inspeccionarlo; nunca se borra.
                let corrupt = self.corrupt_file();
                fs::rename(&self.file, &corrupt).map_err(io_err(&self.file))?;
                match backup {
                    ReadResult::Valid(items) => {
                        self.save(&items)?;
                        Ok((items, LoadOutcome::RecoveredFromBackup))
                    }
                    _ => Ok((Vec::new(), LoadOutcome::ResetCorrupt)),
                }
            }
        }
    }

    /// Carga la lista (con recuperación automática).
    pub fn load(&self) -> Result<Vec<RecentRepository>, RecentsError> {
        Ok(self.load_with_outcome()?.0)
    }

    /// Guarda la lista de forma atómica, conservando copia del contenido válido anterior.
    pub fn save(&self, items: &[RecentRepository]) -> Result<(), RecentsError> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir).map_err(io_err(dir))?;
        }
        let mut json = serde_json::to_vec_pretty(items)?;
        json.push(b'\n');

        let tmp = self.tmp_file();
        {
            let mut handle = fs::File::create(&tmp).map_err(io_err(&tmp))?;
            handle.write_all(&json).map_err(io_err(&tmp))?;
            handle.sync_all().map_err(io_err(&tmp))?;
        }
        // Copia de respaldo solo si lo que hay ahora es válido (no se pisa una buena con una mala).
        if matches!(read_list(&self.file), Ok(ReadResult::Valid(_))) {
            let backup = self.backup_file();
            fs::copy(&self.file, &backup).map_err(io_err(&backup))?;
        }
        fs::rename(&tmp, &self.file).map_err(|source| {
            let _ = fs::remove_file(&tmp);
            RecentsError::Io {
                path: self.file.clone(),
                source,
            }
        })
    }

    /// Registra la apertura de un repo: lo sube al principio conservando favorito y última rama.
    ///
    /// `opened_at` son segundos Unix; usa [`now_unix`] para «ahora». Pasa de [`MAX_RECENTS`]
    /// recortando los más antiguos, pero **nunca** elimina favoritos.
    pub fn remember(
        &self,
        path: &Path,
        branch: Option<&str>,
        opened_at: i64,
    ) -> Result<(), RecentsError> {
        let mut items = self.load()?;
        let path_text = path.to_string_lossy().into_owned();
        let previous = items
            .iter()
            .find(|i| same_path(&i.path, &path_text))
            .cloned();
        items.retain(|i| !same_path(&i.path, &path_text));
        let name = path
            .file_name()
            .map_or_else(|| path_text.clone(), |n| n.to_string_lossy().into_owned());
        items.insert(
            0,
            RecentRepository {
                is_favorite: previous.as_ref().is_some_and(|p| p.is_favorite),
                last_branch: branch
                    .map(str::to_owned)
                    .or_else(|| previous.and_then(|p| p.last_branch)),
                last_opened: opened_at,
                name,
                path: path_text,
            },
        );
        self.save(&trim(items))
    }

    /// Marca o desmarca un favorito. No hace nada si el repo no está en la lista.
    pub fn set_favorite(&self, path: &str, is_favorite: bool) -> Result<(), RecentsError> {
        let mut items = self.load()?;
        let Some(item) = items.iter_mut().find(|i| same_path(&i.path, path)) else {
            return Ok(());
        };
        item.is_favorite = is_favorite;
        self.save(&items)
    }

    /// Quita un repo de la lista. Devuelve `true` si estaba.
    pub fn remove(&self, path: &str) -> Result<bool, RecentsError> {
        let mut items = self.load()?;
        let before = items.len();
        items.retain(|i| !same_path(&i.path, path));
        let removed = items.len() != before;
        if removed {
            self.save(&items)?;
        }
        Ok(removed)
    }

    /// Importa el `recents.json` del Swift (u otro archivo con el mismo formato): añade los
    /// repos que no estén ya; los existentes conservan sus datos salvo que el importado sea más
    /// reciente. Devuelve cuántos repos nuevos se añadieron. Si el archivo no existe o está
    /// corrupto no cambia nada y devuelve 0.
    pub fn import_from(&self, other: &Path) -> Result<usize, RecentsError> {
        let ReadResult::Valid(incoming) = read_list(other)? else {
            return Ok(0);
        };
        let mut items = self.load()?;
        let mut added = 0;
        for item in incoming {
            match items.iter_mut().find(|i| same_path(&i.path, &item.path)) {
                None => {
                    items.push(item);
                    added += 1;
                }
                Some(existing) => {
                    existing.is_favorite |= item.is_favorite;
                    if item.last_opened > existing.last_opened {
                        existing.last_opened = item.last_opened;
                        if item.last_branch.is_some() {
                            existing.last_branch = item.last_branch;
                        }
                    }
                }
            }
        }
        if added > 0 || !items.is_empty() {
            items.sort_by_key(|i| std::cmp::Reverse(i.last_opened));
            self.save(&trim(items))?;
        }
        Ok(added)
    }
}

/// Recorta a [`MAX_RECENTS`] conservando todos los favoritos.
fn trim(items: Vec<RecentRepository>) -> Vec<RecentRepository> {
    if items.len() <= MAX_RECENTS {
        return items;
    }
    let mut kept: Vec<RecentRepository> = items.iter().take(MAX_RECENTS).cloned().collect();
    for fav in items.iter().skip(MAX_RECENTS).filter(|i| i.is_favorite) {
        kept.push(fav.clone());
    }
    kept
}

/// Carpeta de configuración de Ariadna (ver documentación del módulo).
pub fn config_dir() -> Result<PathBuf, RecentsError> {
    let base = directories::BaseDirs::new().ok_or(RecentsError::NoConfigDir)?;
    let name = if cfg!(target_os = "linux") {
        "ariadna"
    } else {
        "Ariadna"
    };
    Ok(base.config_dir().join(name))
}

/// Segundos Unix actuales.
pub fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX))
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    file.with_file_name(name)
}

/// Compara rutas ignorando separadores finales y, en Windows, mayúsculas y `/` vs `\`.
fn same_path(a: &str, b: &str) -> bool {
    fn norm(p: &str) -> String {
        let trimmed = p.trim_end_matches(['/', '\\']);
        if cfg!(windows) {
            trimmed.replace('\\', "/").to_lowercase()
        } else {
            trimmed.to_owned()
        }
    }
    let (a, b) = (norm(a), norm(b));
    a == b || (a.is_empty() && b.is_empty())
}

enum ReadResult {
    Missing,
    Valid(Vec<RecentRepository>),
    Corrupt,
}

fn read_list(file: &Path) -> Result<ReadResult, RecentsError> {
    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(ReadResult::Missing),
        Err(source) => {
            return Err(RecentsError::Io {
                path: file.to_path_buf(),
                source,
            })
        }
    };
    let bytes = bytes
        .strip_prefix(&[0xEF, 0xBB, 0xBF][..])
        .unwrap_or(&bytes);
    Ok(
        match serde_json::from_slice::<Vec<RecentRepository>>(bytes) {
            Ok(items) => ReadResult::Valid(items),
            Err(_) => ReadResult::Corrupt,
        },
    )
}

/// Fechas ISO 8601 como las escribe/lee `JSONEncoder.dateEncodingStrategy = .iso8601` de
/// Swift (`yyyy-MM-dd'T'HH:mm:ss'Z'`). Al leer se aceptan además fracciones y desfases `±HH:MM`.
pub mod iso8601 {
    use serde::{Deserialize, Deserializer, Serializer};

    /// Días desde 1970-01-01 para una fecha civil (algoritmo de Howard Hinnant).
    fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = (m + 9) % 12;
        let doy = (153 * mp + 2) / 5 + d - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    fn civil_from_days(z: i64) -> (i64, i64, i64) {
        let z = z + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let y = yoe + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        (if m <= 2 { y + 1 } else { y }, m, d)
    }

    /// Segundos Unix → `2023-11-14T22:13:20Z`.
    pub fn format(secs: i64) -> String {
        let days = secs.div_euclid(86_400);
        let rem = secs.rem_euclid(86_400);
        let (y, m, d) = civil_from_days(days);
        format!(
            "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
            rem / 3600,
            (rem % 3600) / 60,
            rem % 60
        )
    }

    /// `2023-11-14T22:13:20Z` (u otras variantes ISO 8601) → segundos Unix.
    pub fn parse(text: &str) -> Option<i64> {
        let text = text.trim();
        let (date, time) = text.split_once(['T', 't', ' '])?;
        let mut d = date.split('-');
        let (y, m, day) = (d.next()?, d.next()?, d.next()?);
        if d.next().is_some() || y.len() != 4 || m.len() != 2 || day.len() != 2 {
            return None;
        }
        let (y, m, day): (i64, i64, i64) = (y.parse().ok()?, m.parse().ok()?, day.parse().ok()?);
        if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
            return None;
        }
        // Separa la zona horaria.
        let (clock, offset) = if let Some(c) = time.strip_suffix(['Z', 'z']) {
            (c, 0)
        } else {
            let pos = time.rfind(['+', '-'])?;
            let (c, z) = time.split_at(pos);
            let sign = if z.starts_with('-') { -1 } else { 1 };
            let z = &z[1..];
            let (hh, mm) = match z.split_once(':') {
                Some((h, m)) => (h, m),
                None if z.len() == 4 => z.split_at(2),
                None => (z, "00"),
            };
            (
                c,
                sign * (hh.parse::<i64>().ok()? * 3600 + mm.parse::<i64>().ok()? * 60),
            )
        };
        let clock = clock.split_once('.').map_or(clock, |(whole, _)| whole);
        let mut t = clock.split(':');
        let (hh, mm, ss): (i64, i64, i64) = (
            t.next()?.parse().ok()?,
            t.next()?.parse().ok()?,
            t.next()?.parse().ok()?,
        );
        if t.next().is_some() || hh > 23 || mm > 59 || ss > 60 {
            return None;
        }
        Some(days_from_civil(y, m, day) * 86_400 + hh * 3600 + mm * 60 + ss - offset)
    }

    pub fn serialize<S: Serializer>(secs: &i64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format(*secs))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
        let text = String::deserialize(d)?;
        parse(&text)
            .ok_or_else(|| serde::de::Error::custom(format!("fecha ISO 8601 no válida: {text:?}")))
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    const T0: i64 = 1_700_000_000;

    fn store() -> (tempfile::TempDir, RecentStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = RecentStore::with_base_dir(dir.path().join("Ariadna"));
        (dir, store)
    }

    #[test]
    fn iso8601_matches_swift_format() {
        assert_eq!(iso8601::format(T0), "2023-11-14T22:13:20Z");
        assert_eq!(iso8601::format(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso8601::format(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso8601::parse("2023-11-14T22:13:20Z"), Some(T0));
        assert_eq!(iso8601::parse("2023-11-14T22:13:20.123Z"), Some(T0));
        assert_eq!(iso8601::parse("2023-11-14T23:13:20+01:00"), Some(T0));
        assert_eq!(iso8601::parse("2023-11-14T17:13:20-0500"), Some(T0));
        for secs in [-86_400, -1, 0, 1, T0, 4_102_444_799, 253_402_300_799] {
            assert_eq!(iso8601::parse(&iso8601::format(secs)), Some(secs), "{secs}");
        }
        for bad in [
            "",
            "ayer",
            "2023-13-01T00:00:00Z",
            "2023-11-14",
            "2023-11-14T25:00:00Z",
            "2023-11-14T22:13:20",
        ] {
            assert_eq!(iso8601::parse(bad), None, "{bad:?}");
        }
    }

    /// Mismo caso que `testRecentRepositoryRoundTrip` del Swift.
    #[test]
    fn remember_favorite_and_round_trip() {
        let (_d, store) = store();
        let path = Path::new("/tmp/demo");
        store.remember(path, Some("main"), T0).unwrap();
        store.set_favorite("/tmp/demo", true).unwrap();
        store.remember(path, Some("feature"), T0 + 60).unwrap();
        let loaded = store.load().unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "demo");
        assert_eq!(loaded[0].last_branch.as_deref(), Some("feature"));
        assert!(loaded[0].is_favorite);
        assert_eq!(loaded[0].last_opened, T0 + 60);
    }

    #[test]
    fn remember_keeps_previous_branch_when_none_given() {
        let (_d, store) = store();
        store.remember(Path::new("/r/a"), Some("dev"), T0).unwrap();
        store.remember(Path::new("/r/a"), None, T0 + 1).unwrap();
        assert_eq!(store.load().unwrap()[0].last_branch.as_deref(), Some("dev"));
    }

    #[test]
    fn most_recent_first_and_remove() {
        let (_d, store) = store();
        for (i, name) in ["a", "b", "c"].iter().enumerate() {
            store
                .remember(Path::new(&format!("/r/{name}")), None, T0 + i as i64)
                .unwrap();
        }
        let names: Vec<_> = store.load().unwrap().into_iter().map(|r| r.name).collect();
        assert_eq!(names, ["c", "b", "a"]);
        assert!(store.remove("/r/b").unwrap());
        assert!(!store.remove("/r/b").unwrap());
        assert_eq!(store.load().unwrap().len(), 2);
    }

    #[test]
    fn trimming_keeps_favorites_beyond_the_limit() {
        let (_d, store) = store();
        store.remember(Path::new("/r/favorito"), None, T0).unwrap();
        store.set_favorite("/r/favorito", true).unwrap();
        for i in 1..=40 {
            store
                .remember(Path::new(&format!("/r/repo{i}")), None, T0 + i)
                .unwrap();
        }
        let items = store.load().unwrap();
        assert_eq!(items.len(), MAX_RECENTS + 1);
        assert!(items.iter().any(|r| r.name == "favorito" && r.is_favorite));
        assert_eq!(items[0].name, "repo40");
    }

    #[test]
    fn reads_swift_written_file_and_writes_swift_compatible_json() {
        let (_d, store) = store();
        fs::create_dir_all(store.file().parent().unwrap()).unwrap();
        // Tal como lo escribe JSONEncoder (.prettyPrinted, .sortedKeys, .iso8601).
        let swift = "[\n  {\n    \"isFavorite\" : true,\n    \"lastBranch\" : \"main\",\n    \"lastOpened\" : \"2023-11-14T22:13:20Z\",\n    \"name\" : \"demo\",\n    \"path\" : \"\\/Users\\/ana\\/dev\\/demo\"\n  },\n  {\n    \"isFavorite\" : false,\n    \"lastOpened\" : \"2023-11-13T10:00:00Z\",\n    \"name\" : \"otro\",\n    \"path\" : \"\\/Users\\/ana\\/dev\\/otro\"\n  }\n]";
        fs::write(store.file(), swift).unwrap();
        let items = store.load().unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].path, "/Users/ana/dev/demo");
        assert!(items[0].is_favorite);
        assert_eq!(items[0].last_opened, T0);
        assert_eq!(items[1].last_branch, None);

        store.save(&items).unwrap();
        let text = fs::read_to_string(store.file()).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value[0]["lastOpened"], "2023-11-14T22:13:20Z");
        assert_eq!(value[0]["isFavorite"], true);
        assert!(
            value[1].get("lastBranch").is_none(),
            "lastBranch nulo se omite, como en Swift"
        );
        // Claves ordenadas como `.sortedKeys`.
        let order: Vec<_> = ["isFavorite", "lastBranch", "lastOpened", "name", "path"]
            .iter()
            .map(|k| text.find(k).unwrap())
            .collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn writes_leave_backup_and_no_tmp() {
        let (_d, store) = store();
        store.remember(Path::new("/r/a"), None, T0).unwrap();
        store.remember(Path::new("/r/b"), None, T0 + 1).unwrap();
        let dir = store.file().parent().unwrap();
        assert!(dir.join("recents.json.bak").exists());
        assert!(!dir.join("recents.json.tmp").exists());
        let backup: Vec<RecentRepository> =
            serde_json::from_slice(&fs::read(dir.join("recents.json.bak")).unwrap()).unwrap();
        assert_eq!(backup.len(), 1);
    }

    #[test]
    fn corrupt_main_recovers_from_backup() {
        let (_d, store) = store();
        store.remember(Path::new("/r/a"), None, T0).unwrap();
        store.remember(Path::new("/r/b"), None, T0 + 1).unwrap();
        fs::write(store.file(), b"[{\"path\": truncado").unwrap();
        let (items, outcome) = store.load_with_outcome().unwrap();
        assert_eq!(outcome, LoadOutcome::RecoveredFromBackup);
        assert_eq!(items.len(), 1); // la copia tenía solo `a`
        assert!(store
            .file()
            .parent()
            .unwrap()
            .join("recents.json.corrupt")
            .exists());
        // Tras recuperar, el principal vuelve a ser válido.
        assert_eq!(store.load_with_outcome().unwrap().1, LoadOutcome::Ok);
    }

    #[test]
    fn corrupt_main_without_backup_resets_but_keeps_evidence() {
        let (_d, store) = store();
        fs::create_dir_all(store.file().parent().unwrap()).unwrap();
        fs::write(store.file(), b"\xff\xfe basura").unwrap();
        let (items, outcome) = store.load_with_outcome().unwrap();
        assert!(items.is_empty());
        assert_eq!(outcome, LoadOutcome::ResetCorrupt);
        let corrupt = store.file().parent().unwrap().join("recents.json.corrupt");
        assert_eq!(fs::read(corrupt).unwrap(), b"\xff\xfe basura");
        // Se puede seguir usando.
        store.remember(Path::new("/r/a"), None, T0).unwrap();
        assert_eq!(store.load().unwrap().len(), 1);
    }

    #[test]
    fn missing_file_is_empty_and_main_missing_uses_backup() {
        let (_d, store) = store();
        assert_eq!(
            store.load_with_outcome().unwrap(),
            (vec![], LoadOutcome::Missing)
        );
        store.remember(Path::new("/r/a"), None, T0).unwrap();
        store.remember(Path::new("/r/b"), None, T0 + 1).unwrap();
        fs::remove_file(store.file()).unwrap();
        let (items, outcome) = store.load_with_outcome().unwrap();
        assert_eq!(outcome, LoadOutcome::RecoveredFromBackup);
        assert_eq!(items.len(), 1);
    }

    #[test]
    fn bad_save_does_not_destroy_good_backup() {
        let (_d, store) = store();
        store.remember(Path::new("/r/a"), None, T0).unwrap();
        store.remember(Path::new("/r/b"), None, T0 + 1).unwrap();
        fs::write(store.file(), b"basura").unwrap();
        // Guardar sobre un principal corrupto no debe copiarlo encima de la copia buena.
        store.save(&[]).unwrap();
        let backup: Vec<RecentRepository> = serde_json::from_slice(
            &fs::read(store.file().parent().unwrap().join("recents.json.bak")).unwrap(),
        )
        .unwrap();
        assert_eq!(backup.len(), 1);
    }

    #[test]
    fn imports_swift_file_without_duplicating() {
        let (dir, store) = store();
        store.remember(Path::new("/r/a"), Some("x"), T0).unwrap();
        let swift = dir.path().join("swift-recents.json");
        fs::write(
            &swift,
            r#"[{"isFavorite":true,"lastBranch":"main","lastOpened":"2023-11-15T00:00:00Z","name":"a","path":"/r/a"},
                {"isFavorite":false,"lastOpened":"2020-01-01T00:00:00Z","name":"z","path":"/r/z"}]"#,
        )
        .unwrap();
        assert_eq!(store.import_from(&swift).unwrap(), 1);
        let items = store.load().unwrap();
        assert_eq!(items.len(), 2);
        let a = items.iter().find(|i| i.name == "a").unwrap();
        assert!(a.is_favorite);
        assert_eq!(a.last_branch.as_deref(), Some("main"));
        // Importar un archivo inexistente o corrupto no cambia nada.
        assert_eq!(
            store
                .import_from(&dir.path().join("no-existe.json"))
                .unwrap(),
            0
        );
        fs::write(&swift, b"{{{").unwrap();
        assert_eq!(store.import_from(&swift).unwrap(), 0);
        assert_eq!(store.load().unwrap().len(), 2);
    }

    #[test]
    fn unicode_and_trailing_separator_paths() {
        let (_d, store) = store();
        store
            .remember(Path::new("/r/cañón ñandú"), None, T0)
            .unwrap();
        store
            .remember(Path::new("/r/cañón ñandú/"), None, T0 + 1)
            .unwrap();
        let items = store.load().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].name, "cañón ñandú");
    }

    #[test]
    fn default_location_is_under_ariadna_dir() {
        if let Ok(store) = RecentStore::default_location() {
            let text = store.file().to_string_lossy().to_lowercase();
            assert!(
                text.ends_with("recents.json") && text.contains("ariadna"),
                "{text}"
            );
            if cfg!(target_os = "macos") {
                assert!(text.contains("library/application support/ariadna"));
            }
        }
    }
}
