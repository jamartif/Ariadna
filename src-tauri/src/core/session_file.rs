//! `session.json`: pestañas abiertas y orden. Versionado, escritura atómica (tmp + rename),
//! copia de respaldo `.bak` del contenido válido anterior y recuperación si está corrupto
//! (spec §8). El corrupto se aparta como `.corrupt`; nunca se borra.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Versión actual del formato.
pub const SESSION_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTab {
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionData {
    pub version: u32,
    pub tabs: Vec<SavedTab>,
    /// Índice (en `tabs`) de la pestaña activa.
    pub active: Option<usize>,
}

impl Default for SessionData {
    fn default() -> Self {
        Self {
            version: SESSION_VERSION,
            tabs: Vec::new(),
            active: None,
        }
    }
}

/// Cómo se obtuvo la sesión al cargar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionLoad {
    Missing,
    Ok,
    RecoveredFromBackup,
    ResetCorrupt,
}

#[derive(Debug, Clone)]
pub struct SessionStore {
    file: PathBuf,
}

enum Read {
    Missing,
    Valid(SessionData),
    Corrupt,
}

fn sibling(file: &Path, suffix: &str) -> PathBuf {
    let mut name = file
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    file.with_file_name(name)
}

fn read(file: &Path) -> Read {
    let bytes = match fs::read(file) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Read::Missing,
        Err(_) => return Read::Corrupt,
    };
    match serde_json::from_slice::<SessionData>(&bytes) {
        // Una versión desconocida (de un Ariadna más nuevo) no se interpreta ni se pisa.
        Ok(d) if d.version == SESSION_VERSION => Read::Valid(d),
        _ => Read::Corrupt,
    }
}

impl SessionStore {
    pub fn new(file: impl Into<PathBuf>) -> Self {
        Self { file: file.into() }
    }

    pub fn file(&self) -> &Path {
        &self.file
    }

    pub fn load(&self) -> (SessionData, SessionLoad) {
        let backup = sibling(&self.file, ".bak");
        match read(&self.file) {
            Read::Valid(d) => (d, SessionLoad::Ok),
            Read::Missing => match read(&backup) {
                Read::Valid(d) => (d, SessionLoad::RecoveredFromBackup),
                _ => (SessionData::default(), SessionLoad::Missing),
            },
            Read::Corrupt => {
                let _ = fs::rename(&self.file, sibling(&self.file, ".corrupt"));
                match read(&backup) {
                    Read::Valid(d) => {
                        let _ = self.save(&d);
                        (d, SessionLoad::RecoveredFromBackup)
                    }
                    _ => (SessionData::default(), SessionLoad::ResetCorrupt),
                }
            }
        }
    }

    pub fn save(&self, data: &SessionData) -> std::io::Result<()> {
        if let Some(dir) = self.file.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut json = serde_json::to_vec_pretty(data)?;
        json.push(b'\n');
        let tmp = sibling(&self.file, ".tmp");
        {
            let mut h = fs::File::create(&tmp)?;
            h.write_all(&json)?;
            h.sync_all()?;
        }
        // Respaldo solo de lo válido: no se pisa una copia buena con una mala.
        if matches!(read(&self.file), Read::Valid(_)) {
            fs::copy(&self.file, sibling(&self.file, ".bak"))?;
        }
        fs::rename(&tmp, &self.file).inspect_err(|_| {
            let _ = fs::remove_file(&tmp);
        })
    }
}
