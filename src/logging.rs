use std::{env, fs, fs::OpenOptions, path::PathBuf};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum LoggingError {
    #[error("cannot determine the user state directory")]
    NoStateDirectory,
    #[error("cannot create PIDRA log directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot open PIDRA log {path}: {source}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cannot install PIDRA logger: {0}")]
    Install(String),
}

pub fn initialize() -> Result<PathBuf, LoggingError> {
    let directory = state_directory()?.join("pidra");
    fs::create_dir_all(&directory).map_err(|source| LoggingError::CreateDirectory {
        path: directory.clone(),
        source,
    })?;
    let path = directory.join("pidra.log");
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|source| LoggingError::Open {
            path: path.clone(),
            source,
        })?;
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(true)
        .with_writer(file)
        .try_init()
        .map_err(|error| LoggingError::Install(error.to_string()))?;
    Ok(path)
}

fn state_directory() -> Result<PathBuf, LoggingError> {
    if let Some(path) = env::var_os("XDG_STATE_HOME") {
        return Ok(PathBuf::from(path));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".local/state"))
        .ok_or(LoggingError::NoStateDirectory)
}
