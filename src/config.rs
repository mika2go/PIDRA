use std::{env, fs, path::PathBuf};

use serde::Deserialize;
use thiserror::Error;

use crate::cli::{DEFAULT_REFRESH_MS, MAX_REFRESH_MS, MIN_REFRESH_MS};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorMode {
    Auto,
    Always,
    Never,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub refresh_interval_ms: u64,
    pub mouse: bool,
    pub color: ColorMode,
    pub unicode: bool,
    pub confirm_force_stop: bool,
    pub show_kernel_threads: bool,
    pub mask_command_secrets: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            refresh_interval_ms: DEFAULT_REFRESH_MS,
            mouse: true,
            color: ColorMode::Auto,
            unicode: true,
            confirm_force_stop: true,
            show_kernel_threads: false,
            mask_command_secrets: true,
        }
    }
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("cannot determine the user configuration directory")]
    NoConfigDirectory,
    #[error("cannot read PIDRA config {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid PIDRA config {path}: {reason}")]
    Invalid { path: PathBuf, reason: String },
}

impl Config {
    pub fn load_default() -> Result<(Self, PathBuf), ConfigError> {
        let path = config_path()?;
        if !path.exists() {
            return Ok((Self::default(), path));
        }
        let contents = fs::read_to_string(&path).map_err(|source| ConfigError::Read {
            path: path.clone(),
            source,
        })?;
        let config = parse(&contents).map_err(|reason| ConfigError::Invalid {
            path: path.clone(),
            reason,
        })?;
        Ok((config, path))
    }

    #[must_use]
    pub fn no_color(&self, cli_no_color: bool) -> bool {
        cli_no_color
            || self.color == ColorMode::Never
            || (self.color == ColorMode::Auto && env::var_os("NO_COLOR").is_some())
    }
}

pub fn config_path() -> Result<PathBuf, ConfigError> {
    if let Some(path) = env::var_os("XDG_CONFIG_HOME") {
        return Ok(PathBuf::from(path).join("pidra/config.toml"));
    }
    env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/pidra/config.toml"))
        .ok_or(ConfigError::NoConfigDirectory)
}

fn parse(contents: &str) -> Result<Config, String> {
    let config: Config = toml::from_str(contents).map_err(|error| error.to_string())?;
    if !(MIN_REFRESH_MS..=MAX_REFRESH_MS).contains(&config.refresh_interval_ms) {
        return Err(format!(
            "refresh_interval_ms must be between {MIN_REFRESH_MS} and {MAX_REFRESH_MS}"
        ));
    }
    if !config.confirm_force_stop {
        return Err("confirm_force_stop cannot be disabled by the PIDRA safety policy".to_owned());
    }
    if !config.mask_command_secrets {
        return Err("mask_command_secrets cannot be disabled in this release".to_owned());
    }
    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::{ColorMode, Config, parse};

    #[test]
    fn missing_fields_receive_documented_defaults() {
        let config = parse("refresh_interval_ms = 250\nmouse = false").expect("partial config");

        assert_eq!(config.refresh_interval_ms, 250);
        assert!(!config.mouse);
        assert_eq!(config.color, ColorMode::Auto);
        assert!(config.unicode);
    }

    #[test]
    fn invalid_values_are_clear_and_do_not_mutate_input() {
        let error = parse("refresh_interval_ms = 20").expect_err("invalid refresh");
        assert!(error.contains("between 100 and 60000"));

        let error = parse("confirm_force_stop = false").expect_err("unsafe config");
        assert!(error.contains("cannot be disabled"));
    }

    #[test]
    fn unknown_keys_are_rejected_instead_of_silently_ignored() {
        let error = parse("surprise = true").expect_err("unknown field");
        assert!(error.contains("unknown field"));
    }

    #[test]
    fn defaults_match_the_build_contract() {
        let config = Config::default();
        assert_eq!(config.refresh_interval_ms, 1_000);
        assert!(config.mouse);
        assert!(config.confirm_force_stop);
        assert!(config.mask_command_secrets);
    }
}
