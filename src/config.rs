use crate::Theme;
use anyhow::Result;
use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub theme: Theme,
}

pub fn resolve_config() -> Config {
    match parse_user_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Warning: Failed to load user config: {}", e);
            eprintln!("Using default theme configuration");
            Config {
                theme: Theme::default(),
            }
        }
    }
}

fn parse_user_config() -> Result<Config> {
    if let Some(config_dir) = dirs::config_dir() {
        let conf_path = config_dir.join("FastSSH");
        let config_file = conf_path.join("config.yaml");

        fs::create_dir_all(&conf_path)?;

        if !config_file.exists() {
            fs::write(&config_file, DEFAULT_CONFIG)?;
        }

        let config_file_text = fs::read_to_string(&config_file)?;

        let conf = match serde_yaml::from_str::<Option<Config>>(&config_file_text) {
            Ok(Some(conf)) => conf,
            Ok(None) => {
                return Err(anyhow::anyhow!("Config file is empty or invalid"));
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "Error parsing config file, make sure format is valid: {}",
                    e
                ));
            }
        };

        return Ok(conf);
    }

    Err(anyhow::anyhow!("Could not get config directory"))
}

const DEFAULT_CONFIG: &str = "
# This is the default configuration for FastSSH.

theme:
    text_primary: \"#b967ff\"
    text_secondary: \"#ffffff\"
    border_color: \"#b967ff\"
";
