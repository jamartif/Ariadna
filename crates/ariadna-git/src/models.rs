//! Modelos de datos del núcleo git (réplica de `Models.swift`).
//!
//! Las rutas que produce git se guardan como [`GitPath`] (bytes), nunca como `String`,
//! para no perder nombres que no son UTF-8. Los textos de diff y de conflicto se
//! conservan como bytes y solo se serializan (hacia la UI) con conversión con pérdida.

use std::borrow::Cow;
use std::ffi::OsString;
use std::fmt;
use std::path::PathBuf;

use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

// ---------------------------------------------------------------------------
// GitPath
// ---------------------------------------------------------------------------

/// Ruta relativa al repo tal y como la emite git: bytes con `/` como separador.
///
/// En JSON se serializa como cadena si es UTF-8 válido y como lista de bytes si no,
/// de modo que el round-trip por IPC no pierde información.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct GitPath(Vec<u8>);

impl GitPath {
    /// Crea la ruta a partir de bytes.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Bytes originales.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Texto para mostrar (con pérdida si hay bytes no UTF-8).
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    /// Convierte a `OsString` sin pérdida en Unix; en Windows (donde git emite UTF-8)
    /// los bytes inválidos se sustituyen.
    pub fn to_os_string(&self) -> OsString {
        bytes_to_os_string(&self.0)
    }

    /// Igual que [`GitPath::to_os_string`] pero como `PathBuf`.
    pub fn to_path_buf(&self) -> PathBuf {
        PathBuf::from(self.to_os_string())
    }

    /// `true` si la ruta está vacía.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Bytes → `OsString`. Sin pérdida en Unix.
pub fn bytes_to_os_string(bytes: &[u8]) -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(bytes).to_os_string()
    }
    #[cfg(not(unix))]
    {
        OsString::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

impl fmt::Debug for GitPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "GitPath({:?})", self.to_string_lossy())
    }
}

impl fmt::Display for GitPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_lossy())
    }
}

impl From<&str> for GitPath {
    fn from(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }
}
impl From<String> for GitPath {
    fn from(s: String) -> Self {
        Self(s.into_bytes())
    }
}
impl From<Vec<u8>> for GitPath {
    fn from(b: Vec<u8>) -> Self {
        Self(b)
    }
}
impl From<&[u8]> for GitPath {
    fn from(b: &[u8]) -> Self {
        Self(b.to_vec())
    }
}
impl PartialEq<&str> for GitPath {
    fn eq(&self, other: &&str) -> bool {
        self.0 == other.as_bytes()
    }
}
impl PartialEq<str> for GitPath {
    fn eq(&self, other: &str) -> bool {
        self.0 == other.as_bytes()
    }
}

impl Serialize for GitPath {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(&self.0) {
            Ok(text) => serializer.serialize_str(text),
            Err(_) => serializer.collect_seq(self.0.iter()),
        }
    }
}

impl<'de> Deserialize<'de> for GitPath {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PathVisitor;
        impl<'de> Visitor<'de> for PathVisitor {
            type Value = GitPath;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("una cadena o una lista de bytes")
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<GitPath, E> {
                Ok(GitPath(v.as_bytes().to_vec()))
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<GitPath, E> {
                Ok(GitPath(v.to_vec()))
            }
            fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<GitPath, E> {
                Ok(GitPath(v))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<GitPath, A::Error> {
                let mut out = Vec::new();
                while let Some(b) = seq.next_element::<u8>()? {
                    out.push(b);
                }
                Ok(GitPath(out))
            }
        }
        deserializer.deserialize_any(PathVisitor)
    }
}

/// Serializa bytes como texto UTF-8 con pérdida (solo para la UI).
pub(crate) fn ser_lossy<S: Serializer>(bytes: &[u8], s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&String::from_utf8_lossy(bytes))
}

/// Como [`ser_lossy`] para `Option<Vec<u8>>`.
pub(crate) fn ser_lossy_opt<S: Serializer>(
    bytes: &Option<Vec<u8>>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match bytes {
        Some(b) => s.serialize_str(&String::from_utf8_lossy(b)),
        None => s.serialize_none(),
    }
}

// ---------------------------------------------------------------------------
// Estado
// ---------------------------------------------------------------------------

/// Letra de estado de `git status --porcelain=v2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ChangeStatus {
    #[serde(rename = ".")]
    Unmodified,
    #[serde(rename = "M")]
    Modified,
    #[serde(rename = "A")]
    Added,
    #[serde(rename = "D")]
    Deleted,
    #[serde(rename = "R")]
    Renamed,
    #[serde(rename = "C")]
    Copied,
    #[serde(rename = "U")]
    Unmerged,
    #[serde(rename = "T")]
    TypeChanged,
    #[serde(rename = "?")]
    Untracked,
    #[serde(rename = "!")]
    Ignored,
    #[serde(rename = "")]
    Unknown,
}

impl ChangeStatus {
    /// Interpreta el byte de porcelain; cualquier otro valor es `Unknown`.
    pub fn from_porcelain(byte: u8) -> Self {
        match byte {
            b'.' => Self::Unmodified,
            b'M' => Self::Modified,
            b'A' => Self::Added,
            b'D' => Self::Deleted,
            b'R' => Self::Renamed,
            b'C' => Self::Copied,
            b'U' => Self::Unmerged,
            b'T' => Self::TypeChanged,
            b'?' => Self::Untracked,
            b'!' => Self::Ignored,
            _ => Self::Unknown,
        }
    }
}

/// Cómo está un archivo en conflicto, según el par XY.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictKind {
    BothModified,
    BothAdded,
    BothDeleted,
    AddedByUs,
    AddedByThem,
    DeletedByUs,
    DeletedByThem,
    Unknown,
}

impl ConflictKind {
    /// Interpreta el par XY de una entrada `u`.
    pub fn from_porcelain_xy(xy: &[u8]) -> Self {
        match xy {
            b"UU" => Self::BothModified,
            b"AA" => Self::BothAdded,
            b"DD" => Self::BothDeleted,
            b"AU" => Self::AddedByUs,
            b"UA" => Self::AddedByThem,
            b"DU" => Self::DeletedByUs,
            b"UD" => Self::DeletedByThem,
            _ => Self::Unknown,
        }
    }
}

/// Una entrada de `git status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusEntry {
    pub path: GitPath,
    /// Ruta anterior en renombrados/copias.
    pub original_path: Option<GitPath>,
    pub index_status: ChangeStatus,
    pub work_tree_status: ChangeStatus,
    pub is_conflicted: bool,
    pub is_untracked: bool,
    pub is_ignored: bool,
    pub is_submodule: bool,
    pub conflict_kind: Option<ConflictKind>,
}

impl StatusEntry {
    /// Tiene cambios en el índice (sección «preparados»).
    pub fn is_staged(&self) -> bool {
        !self.is_untracked
            && !self.is_ignored
            && !self.is_conflicted
            && self.index_status != ChangeStatus::Unmodified
    }

    /// Tiene cambios sin preparar (incluye los archivos sin seguimiento).
    pub fn has_unstaged_change(&self) -> bool {
        self.is_untracked
            || (!self.is_ignored
                && !self.is_conflicted
                && self.work_tree_status != ChangeStatus::Unmodified)
    }
}

/// Resultado completo de `git status --porcelain=v2 -z --branch`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub branch_name: Option<String>,
    pub head_oid: Option<String>,
    pub is_detached: bool,
    pub is_unborn: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub entries: Vec<StatusEntry>,
}

impl GitStatus {
    /// Entradas con cambios preparados.
    pub fn staged(&self) -> Vec<&StatusEntry> {
        self.entries.iter().filter(|e| e.is_staged()).collect()
    }
    /// Entradas con cambios sin preparar o sin seguimiento.
    pub fn unstaged(&self) -> Vec<&StatusEntry> {
        self.entries
            .iter()
            .filter(|e| e.has_unstaged_change())
            .collect()
    }
    /// Entradas en conflicto.
    pub fn conflicted(&self) -> Vec<&StatusEntry> {
        self.entries.iter().filter(|e| e.is_conflicted).collect()
    }
    /// Entradas ignoradas.
    pub fn ignored(&self) -> Vec<&StatusEntry> {
        self.entries.iter().filter(|e| e.is_ignored).collect()
    }
    /// Aviso para la barra de commit cuando HEAD está separado.
    pub fn detached_commit_warning(&self) -> Option<&'static str> {
        self.is_detached.then_some(
            "HEAD no apunta a una rama. Si confirmas, el commit no pertenecerá a ninguna rama hasta que crees una.",
        )
    }
}

// ---------------------------------------------------------------------------
// Historial, ramas, remotos
// ---------------------------------------------------------------------------

/// Commit del historial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Commit {
    pub sha: String,
    pub parents: Vec<String>,
    pub subject: String,
    pub body: String,
    pub author_name: String,
    pub author_email: String,
    pub author_date_iso: String,
    pub refs: Vec<String>,
}

impl Commit {
    /// Primeros 7 caracteres del hash.
    pub fn short_sha(&self) -> &str {
        let end = self
            .sha
            .char_indices()
            .nth(7)
            .map_or(self.sha.len(), |(i, _)| i);
        &self.sha[..end]
    }
}

/// Rama local o remota.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Branch {
    pub name: String,
    pub oid: String,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
}

/// Remoto con sus URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

/// Política de avance rápido en un merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeFastForward {
    /// `--ff`
    Allow,
    /// `--ff-only`
    Only,
    /// `--no-ff`
    Never,
}

/// Resultado de un merge.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MergeResult {
    UpToDate,
    FastForward { new_head: String },
    Merged { new_head: String },
    Conflicts { paths: Vec<GitPath> },
}

/// Operación de integración en curso.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IntegrationState {
    None,
    Merging,
    Rebasing,
    CherryPicking,
}

/// Qué diff se pide.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffScope {
    Unstaged,
    Staged,
    Commit(String),
    Between { base: String, head: String },
}

// ---------------------------------------------------------------------------
// Diff
// ---------------------------------------------------------------------------

/// Tipo de línea dentro de un hunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DiffLineKind {
    Context,
    Addition,
    Deletion,
    /// Línea `\ No newline at end of file`.
    Meta,
}

/// Línea de un hunk. `text` no incluye el prefijo ni el `\n`, pero **conserva el `\r`**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    pub id: String,
    pub kind: DiffLineKind,
    pub text: String,
    pub old_line_number: Option<u32>,
    pub new_line_number: Option<u32>,
}

/// Hunk de un diff unificado.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub id: String,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub heading: String,
    pub lines: Vec<DiffLine>,
    /// Bytes exactos del hunk (línea `@@` incluida), listos para recomponer un parche.
    #[serde(skip)]
    pub raw: Vec<u8>,
}

impl DiffHunk {
    /// ¿Mismos rangos que otro hunk? (para reconocer el hunk tras refrescar).
    pub fn matches_range(&self, other: &DiffHunk) -> bool {
        self.old_start == other.old_start
            && self.old_count == other.old_count
            && self.new_start == other.new_start
            && self.new_count == other.new_count
    }
}

/// Diff de un archivo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    /// Ruta nueva (o la antigua si el archivo se borra).
    pub path: GitPath,
    /// Ruta antigua si difiere de `path` (renombrado/copia).
    pub old_path: Option<GitPath>,
    pub is_binary: bool,
    pub is_new_file: bool,
    pub is_deleted_file: bool,
    /// Bytes desde `diff --git` hasta justo antes del primer `@@` (todo el bloque si es binario).
    #[serde(skip)]
    pub header: Vec<u8>,
    pub hunks: Vec<DiffHunk>,
    /// Bytes tras el último hunk que no pertenecen a ninguno (p. ej. firma de `format-patch`).
    #[serde(skip)]
    pub trailer: Vec<u8>,
}

// ---------------------------------------------------------------------------
// Conflictos
// ---------------------------------------------------------------------------

/// Lado elegido al resolver un conflicto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConflictChoice {
    Ours,
    Base,
    Theirs,
}

/// Un lado del modelo a tres vías (contenido del índice: `:1:` base, `:2:` ours, `:3:` theirs).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictSide {
    pub oid: Option<String>,
    #[serde(serialize_with = "ser_lossy_opt")]
    pub content: Option<Vec<u8>>,
    pub is_binary: bool,
}

impl ConflictSide {
    /// Lado ausente.
    pub fn absent() -> Self {
        Self::default()
    }
    /// ¿Existe este lado en el índice?
    pub fn is_present(&self) -> bool {
        self.oid.is_some()
    }
}

/// Región con marcadores de conflicto.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictHunk {
    pub id: usize,
    #[serde(serialize_with = "ser_lossy")]
    pub ours: Vec<u8>,
    /// `None` si el archivo no usa estilo diff3.
    #[serde(serialize_with = "ser_lossy_opt")]
    pub base: Option<Vec<u8>>,
    #[serde(serialize_with = "ser_lossy")]
    pub theirs: Vec<u8>,
}

/// Archivo en conflicto con sus tres lados.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictFile {
    pub path: GitPath,
    pub kind: ConflictKind,
    pub ours: ConflictSide,
    pub base: ConflictSide,
    pub theirs: ConflictSide,
    #[serde(serialize_with = "ser_lossy_opt")]
    pub working_tree: Option<Vec<u8>>,
    pub hunks: Vec<ConflictHunk>,
}

impl ConflictFile {
    /// Contenido completo de un lado del índice. No escribe en disco.
    pub fn taking_whole_side(&self, choice: ConflictChoice) -> Vec<u8> {
        let side = match choice {
            ConflictChoice::Ours => &self.ours,
            ConflictChoice::Base => &self.base,
            ConflictChoice::Theirs => &self.theirs,
        };
        side.content.clone().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Repositorio y recientes
// ---------------------------------------------------------------------------

/// Carpeta que ya es un repo (`git rev-parse` tuvo éxito).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repository {
    pub path: PathBuf,
    pub git_dir: PathBuf,
}

impl Repository {
    /// Nombre de la carpeta raíz.
    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
    }
}

/// Repositorio reciente. Formato idéntico al `recents.json` del Swift:
/// claves `path`, `name`, `lastOpened` (ISO 8601 UTC, segundos), `isFavorite`, `lastBranch`.
///
/// `path` es `String` porque el formato de archivo es JSON/UTF-8 y debe seguir
/// siendo compatible con el Swift (desviación consciente del «nunca String» del §3.3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentRepository {
    pub is_favorite: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_branch: Option<String>,
    /// Segundos Unix (UTC). En JSON: `2023-11-14T22:13:20Z`.
    #[serde(with = "crate::recents::iso8601")]
    pub last_opened: i64,
    pub name: String,
    pub path: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn git_path_round_trips_utf8_and_non_utf8_through_json() {
        let utf8 = GitPath::from("notas/cañón.txt");
        let json = serde_json::to_string(&utf8).unwrap();
        assert_eq!(json, "\"notas/cañón.txt\"");
        assert_eq!(serde_json::from_str::<GitPath>(&json).unwrap(), utf8);

        let raw = GitPath::new(vec![b'a', 0xFF, 0xFE, b'z']);
        let json = serde_json::to_string(&raw).unwrap();
        assert_eq!(json, "[97,255,254,122]");
        assert_eq!(serde_json::from_str::<GitPath>(&json).unwrap(), raw);
    }

    #[test]
    fn staged_and_unstaged_classification() {
        let mut e = StatusEntry {
            path: "f".into(),
            original_path: None,
            index_status: ChangeStatus::Modified,
            work_tree_status: ChangeStatus::Modified,
            is_conflicted: false,
            is_untracked: false,
            is_ignored: false,
            is_submodule: false,
            conflict_kind: None,
        };
        assert!(e.is_staged() && e.has_unstaged_change());
        e.is_conflicted = true;
        assert!(!e.is_staged() && !e.has_unstaged_change());
    }

    #[test]
    fn change_status_serializes_as_porcelain_letter() {
        assert_eq!(
            serde_json::to_string(&ChangeStatus::Modified).unwrap(),
            "\"M\""
        );
        assert_eq!(
            serde_json::to_string(&ChangeStatus::Unmodified).unwrap(),
            "\".\""
        );
        assert_eq!(ChangeStatus::from_porcelain(b'Z'), ChangeStatus::Unknown);
    }

    #[test]
    fn short_sha_handles_short_input() {
        let mut c = Commit {
            sha: "abcdef0123".into(),
            parents: vec![],
            subject: String::new(),
            body: String::new(),
            author_name: String::new(),
            author_email: String::new(),
            author_date_iso: String::new(),
            refs: vec![],
        };
        assert_eq!(c.short_sha(), "abcdef0");
        c.sha = "abc".into();
        assert_eq!(c.short_sha(), "abc");
    }

    #[test]
    fn whole_side_returns_content_or_empty() {
        let file = ConflictFile {
            path: "f".into(),
            kind: ConflictKind::BothModified,
            ours: ConflictSide {
                oid: Some("a".into()),
                content: Some(b"o\r\n".to_vec()),
                is_binary: false,
            },
            base: ConflictSide::absent(),
            theirs: ConflictSide {
                oid: Some("c".into()),
                content: Some(b"t".to_vec()),
                is_binary: false,
            },
            working_tree: None,
            hunks: vec![],
        };
        assert_eq!(file.taking_whole_side(ConflictChoice::Ours), b"o\r\n");
        assert!(file.taking_whole_side(ConflictChoice::Base).is_empty());
        assert!(!file.base.is_present());
    }
}
