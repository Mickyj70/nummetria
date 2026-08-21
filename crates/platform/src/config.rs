use std::ffi::OsString;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{PathError, PlatformPaths};

pub const CONFIG_VERSION: u16 = 1;
pub const CONFIG_ENV: &str = "NUMMETRIA_CONFIG";
pub const DATABASE_ENV: &str = "NUMMETRIA_DATABASE";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AppConfig {
    pub config_version: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub database_path: Option<PathBuf>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            config_version: CONFIG_VERSION,
            database_path: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSource {
    CommandLine,
    Environment,
    Configuration,
    Default,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveOptions {
    pub config_path: Option<PathBuf>,
    pub database_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnvironmentOverrides {
    pub config_path: Option<PathBuf>,
    pub database_path: Option<PathBuf>,
}

impl EnvironmentOverrides {
    pub fn from_process() -> Result<Self, ConfigError> {
        Ok(Self {
            config_path: environment_path(CONFIG_ENV, std::env::var_os(CONFIG_ENV))?,
            database_path: environment_path(DATABASE_ENV, std::env::var_os(DATABASE_ENV))?,
        })
    }

    pub fn new(
        config_path: Option<OsString>,
        database_path: Option<OsString>,
    ) -> Result<Self, ConfigError> {
        Ok(Self {
            config_path: environment_path(CONFIG_ENV, config_path)?,
            database_path: environment_path(DATABASE_ENV, database_path)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolvedConfig {
    pub config_path: PathBuf,
    pub config_source: ConfigSource,
    pub config_exists: bool,
    pub database_path: PathBuf,
    pub database_source: ConfigSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetupOutcome {
    Created,
    AlreadyPresent,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error(transparent)]
    Paths(#[from] PathError),
    #[error("{name} cannot be empty")]
    EmptyEnvironment { name: &'static str },
    #[error("explicit configuration file does not exist: {0}")]
    ExplicitConfigMissing(String),
    #[error("could not read configuration file {path}: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid configuration file {path}: {source}")]
    Parse {
        path: String,
        #[source]
        source: toml::de::Error,
    },
    #[error("unsupported configuration version {found}; this build supports version {supported}")]
    UnsupportedVersion { found: u16, supported: u16 },
    #[error("could not serialize configuration: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error("could not create configuration directory {path}: {source}")]
    CreateDirectory {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("could not write configuration file {path}: {source}")]
    Write {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

pub fn resolve_config(
    options: &ResolveOptions,
    environment: &EnvironmentOverrides,
    paths: &PlatformPaths,
) -> Result<ResolvedConfig, ConfigError> {
    let (config_path, config_source, required) = if let Some(path) = &options.config_path {
        (path.clone(), ConfigSource::CommandLine, true)
    } else if let Some(path) = &environment.config_path {
        (path.clone(), ConfigSource::Environment, true)
    } else {
        (paths.config_file(), ConfigSource::Default, false)
    };

    let config_exists = config_path.exists();
    let config = if config_exists {
        load_config(&config_path)?
    } else if required {
        return Err(ConfigError::ExplicitConfigMissing(
            config_path.display().to_string(),
        ));
    } else {
        AppConfig::default()
    };

    let (database_path, database_source) = if let Some(path) = &options.database_path {
        (path.clone(), ConfigSource::CommandLine)
    } else if let Some(path) = &environment.database_path {
        (path.clone(), ConfigSource::Environment)
    } else if let Some(path) = config.database_path {
        (
            resolve_from_config(&config_path, path),
            ConfigSource::Configuration,
        )
    } else {
        (paths.database_file(), ConfigSource::Default)
    };

    Ok(ResolvedConfig {
        config_path,
        config_source,
        config_exists,
        database_path,
        database_source,
    })
}

pub fn write_initial_config(paths: &PlatformPaths) -> Result<SetupOutcome, ConfigError> {
    paths
        .create_directories()
        .map_err(|source| ConfigError::CreateDirectory {
            path: paths.config_dir().display().to_string(),
            source,
        })?;

    let destination = paths.config_file();
    if destination.exists() {
        return Ok(SetupOutcome::AlreadyPresent);
    }

    let contents = toml::to_string_pretty(&AppConfig::default())?;
    let mut temporary = tempfile::NamedTempFile::new_in(paths.config_dir()).map_err(|source| {
        ConfigError::Write {
            path: destination.display().to_string(),
            source,
        }
    })?;
    temporary
        .write_all(contents.as_bytes())
        .and_then(|()| temporary.flush())
        .map_err(|source| ConfigError::Write {
            path: destination.display().to_string(),
            source,
        })?;

    match temporary.persist_noclobber(&destination) {
        Ok(_) => Ok(SetupOutcome::Created),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            Ok(SetupOutcome::AlreadyPresent)
        }
        Err(error) => Err(ConfigError::Write {
            path: destination.display().to_string(),
            source: error.error,
        }),
    }
}

fn load_config(path: &Path) -> Result<AppConfig, ConfigError> {
    let contents = std::fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let config: AppConfig = toml::from_str(&contents).map_err(|source| ConfigError::Parse {
        path: path.display().to_string(),
        source,
    })?;
    if config.config_version != CONFIG_VERSION {
        return Err(ConfigError::UnsupportedVersion {
            found: config.config_version,
            supported: CONFIG_VERSION,
        });
    }
    Ok(config)
}

fn environment_path(
    name: &'static str,
    value: Option<OsString>,
) -> Result<Option<PathBuf>, ConfigError> {
    value
        .map(|value| {
            if value.is_empty() {
                Err(ConfigError::EmptyEnvironment { name })
            } else {
                Ok(PathBuf::from(value))
            }
        })
        .transpose()
}

fn resolve_from_config(config_path: &Path, database_path: PathBuf) -> PathBuf {
    if database_path.is_absolute() {
        database_path
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(database_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(directory: &tempfile::TempDir) -> PlatformPaths {
        PlatformPaths::from_directories(
            directory.path().join("config"),
            directory.path().join("data"),
        )
    }

    fn write_config(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn missing_default_config_uses_default_database() {
        let directory = tempfile::tempdir().unwrap();
        let paths = paths(&directory);
        let resolved = resolve_config(
            &ResolveOptions::default(),
            &EnvironmentOverrides::default(),
            &paths,
        )
        .unwrap();

        assert!(!resolved.config_exists);
        assert_eq!(resolved.config_source, ConfigSource::Default);
        assert_eq!(resolved.database_path, paths.database_file());
        assert_eq!(resolved.database_source, ConfigSource::Default);
    }

    #[test]
    fn applies_cli_environment_config_and_default_precedence() {
        let directory = tempfile::tempdir().unwrap();
        let paths = paths(&directory);
        write_config(
            &paths.config_file(),
            "config_version = 1\ndatabase_path = 'from-config.db'\n",
        );
        let environment = EnvironmentOverrides::new(
            None,
            Some(OsString::from(directory.path().join("from-env.db"))),
        )
        .unwrap();
        let cli_database = directory.path().join("from-cli.db");

        let from_config = resolve_config(
            &ResolveOptions::default(),
            &EnvironmentOverrides::default(),
            &paths,
        )
        .unwrap();
        assert_eq!(
            from_config.database_path,
            paths.config_dir().join("from-config.db")
        );
        assert_eq!(from_config.database_source, ConfigSource::Configuration);

        let from_environment =
            resolve_config(&ResolveOptions::default(), &environment, &paths).unwrap();
        assert_eq!(
            from_environment.database_path,
            directory.path().join("from-env.db")
        );
        assert_eq!(from_environment.database_source, ConfigSource::Environment);

        let from_cli = resolve_config(
            &ResolveOptions {
                config_path: None,
                database_path: Some(cli_database.clone()),
            },
            &environment,
            &paths,
        )
        .unwrap();
        assert_eq!(from_cli.database_path, cli_database);
        assert_eq!(from_cli.database_source, ConfigSource::CommandLine);
    }

    #[test]
    fn rejects_missing_explicit_invalid_unknown_and_future_configs() {
        let directory = tempfile::tempdir().unwrap();
        let paths = paths(&directory);
        let missing = directory.path().join("missing.toml");
        assert!(matches!(
            resolve_config(
                &ResolveOptions {
                    config_path: Some(missing),
                    database_path: None,
                },
                &EnvironmentOverrides::default(),
                &paths,
            ),
            Err(ConfigError::ExplicitConfigMissing(_))
        ));

        for contents in [
            "not valid toml = [",
            "config_version = 1\nsecret = 'must-not-be-accepted'\n",
        ] {
            write_config(&paths.config_file(), contents);
            assert!(matches!(
                resolve_config(
                    &ResolveOptions::default(),
                    &EnvironmentOverrides::default(),
                    &paths,
                ),
                Err(ConfigError::Parse { .. })
            ));
        }

        write_config(&paths.config_file(), "config_version = 2\n");
        assert!(matches!(
            resolve_config(
                &ResolveOptions::default(),
                &EnvironmentOverrides::default(),
                &paths,
            ),
            Err(ConfigError::UnsupportedVersion { found: 2, .. })
        ));
    }

    #[test]
    fn rejects_empty_environment_paths() {
        assert!(matches!(
            EnvironmentOverrides::new(Some(OsString::new()), None),
            Err(ConfigError::EmptyEnvironment { name: CONFIG_ENV })
        ));
        assert!(matches!(
            EnvironmentOverrides::new(None, Some(OsString::new())),
            Err(ConfigError::EmptyEnvironment { name: DATABASE_ENV })
        ));
    }

    #[test]
    fn setup_writes_once_without_overwriting() {
        let directory = tempfile::tempdir().unwrap();
        let paths = paths(&directory);
        assert_eq!(write_initial_config(&paths).unwrap(), SetupOutcome::Created);
        let original = std::fs::read_to_string(paths.config_file()).unwrap();
        assert!(original.contains("config_version = 1"));

        std::fs::write(paths.config_file(), "config_version = 1\n# user edit\n").unwrap();
        assert_eq!(
            write_initial_config(&paths).unwrap(),
            SetupOutcome::AlreadyPresent
        );
        assert!(
            std::fs::read_to_string(paths.config_file())
                .unwrap()
                .contains("# user edit")
        );
    }
}
