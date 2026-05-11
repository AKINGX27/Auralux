use anyhow::Result;
use directories::ProjectDirs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct AuraluxPaths {
    pub data_dir: PathBuf,
    pub database_path: PathBuf,
}

impl AuraluxPaths {
    pub fn resolve(explicit_data_dir: Option<PathBuf>) -> Result<Self> {
        let data_dir = match explicit_data_dir {
            Some(path) => path,
            None => ProjectDirs::from("org", "Auralux", "Auralux")
                .map(|dirs| dirs.data_local_dir().to_path_buf())
                .unwrap_or_else(|| PathBuf::from(".auralux")),
        };
        std::fs::create_dir_all(&data_dir)?;
        let database_path = data_dir.join("library.db");
        Ok(Self {
            data_dir,
            database_path,
        })
    }
}
