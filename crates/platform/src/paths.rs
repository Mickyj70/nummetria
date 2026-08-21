use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use thiserror::Error;

const CONFIG_FILE_NAME: &str = "config.toml";
const DATABASE_FILE_NAME: &str = "nummetria.db";

#[derive(Debug, Error)]
pub enum PathError {
    #[error("could not determine standard application directories for this user")]
    Unavailable,
}

/// Standard locations used by Nummetria on the current operating system.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlatformPaths {
    config_dir: PathBuf,
    data_dir: PathBuf,
}

impl PlatformPaths {
    /// Discovers OS-standard directories for the current user.
    pub fn discover() -> Result<Self, PathError> {
        let project =
            ProjectDirs::from("dev", "Nummetria", "Nummetria").ok_or(PathError::Unavailable)?;
        Ok(Self {
            config_dir: project.config_dir().to_owned(),
            data_dir: project.data_local_dir().to_owned(),
        })
    }

    /// Creates explicit locations for deterministic tests and embedding hosts.
    pub fn from_directories(config_dir: impl Into<PathBuf>, data_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_dir: config_dir.into(),
            data_dir: data_dir.into(),
        }
    }

    pub fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join(CONFIG_FILE_NAME)
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn database_file(&self) -> PathBuf {
        self.data_dir.join(DATABASE_FILE_NAME)
    }

    /// Creates only the standard directories, never configuration or data.
    pub fn create_directories(&self) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(&self.config_dir)?;
        std::fs::create_dir_all(&self.data_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_directories_produce_stable_file_names() {
        let paths = PlatformPaths::from_directories("settings", "local data");
        assert_eq!(paths.config_file(), Path::new("settings/config.toml"));
        assert_eq!(paths.database_file(), Path::new("local data/nummetria.db"));
    }

    #[test]
    fn creates_config_and_data_directories_without_creating_files() {
        let directory = tempfile::tempdir().unwrap();
        let paths = PlatformPaths::from_directories(
            directory.path().join("config folder"),
            directory.path().join("data Ω"),
        );

        paths.create_directories().unwrap();

        assert!(paths.config_dir().is_dir());
        assert!(paths.data_dir().is_dir());
        assert!(!paths.config_file().exists());
        assert!(!paths.database_file().exists());
    }

    #[test]
    fn discovers_paths_for_the_current_supported_platform() {
        let paths = PlatformPaths::discover().unwrap();
        assert!(paths.config_file().ends_with(CONFIG_FILE_NAME));
        assert!(paths.database_file().ends_with(DATABASE_FILE_NAME));
        assert!(paths.config_file().is_absolute());
        assert!(paths.database_file().is_absolute());
    }
}
