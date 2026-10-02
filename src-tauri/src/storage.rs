use crate::models::*;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct Storage {
    pub root: PathBuf,
}
impl Storage {
    pub fn new(root: PathBuf) -> Result<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }
    pub fn load_settings(&self) -> Result<Settings> {
        let p = self.root.join("config.json");
        if !p.exists() {
            return Ok(Settings::default());
        }
        let s: Settings = serde_json::from_slice(&fs::read(p)?)?;
        if s.version != 1 {
            return Err(AppError::new(
                "version",
                "Unsupported settings version.",
                "Use a compatible lazysync version.",
            ));
        }
        Ok(s)
    }
    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.write("config.json", s)
    }
    pub fn journal(&self) -> Result<Option<Recovery>> {
        let p = self.root.join("recovery.json");
        if !p.exists() {
            return Ok(None);
        }
        let r: Recovery = serde_json::from_slice(&fs::read(p)?)?;
        if r.version != 1 {
            return Err(AppError::new(
                "version",
                "Unsupported recovery version.",
                "Use a compatible lazysync version.",
            ));
        }
        Ok(if r.completed { None } else { Some(r) })
    }
    pub fn save_journal(&self, r: &Recovery) -> Result<()> {
        self.write("recovery.json", r)
    }
    pub fn write<T: serde::Serialize>(&self, name: &str, data: &T) -> Result<()> {
        atomic_write(&self.root.join(name), &serde_json::to_vec_pretty(data)?)
    }
    pub fn conflict_folder(&self, r: &Recovery) -> Result<PathBuf> {
        let p = self.root.join("recovery").join(&r.operation_id);
        fs::create_dir_all(&p)?;
        Ok(p)
    }
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::new("path", "Invalid data path.", "Choose a valid folder."))?;
    fs::create_dir_all(parent)?;
    let mut tmp = tempfile::NamedTempFile::new_in(parent)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|_| {
        AppError::new(
            "storage",
            "Could not atomically save app data.",
            "Check permissions and disk space.",
        )
    })?;
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
