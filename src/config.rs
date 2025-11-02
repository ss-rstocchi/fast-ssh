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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_config_returns_default_on_error() {
        // Even if config file doesn't exist or is invalid, should return default
        let config = resolve_config();
        // Just verify it doesn't panic and returns a Config
        let _ = config.theme.text_primary();
    }

    #[test]
    fn test_default_config_is_valid_yaml() {
        // Test that the default config string parses correctly
        let result = serde_yaml::from_str::<Option<Config>>(DEFAULT_CONFIG);
        assert!(result.is_ok());
        
        if let Ok(Some(config)) = result {
            // Verify default theme values can be accessed
            let _ = config.theme.text_primary();
            let _ = config.theme.text_secondary();
            let _ = config.theme.border_color();
        }
    }

    #[test]
    fn test_config_with_custom_theme() {
        let yaml = "theme:\n    text_primary: \"#ff0000\"\n    text_secondary: \"#00ff00\"\n    border_color: \"#0000ff\"";
        let config: Option<Config> = serde_yaml::from_str(yaml).expect("Failed to parse config");
        assert!(config.is_some());
        
        let config = config.unwrap();
        assert_eq!(config.theme.text_primary(), tui::style::Color::Rgb(255, 0, 0));
        assert_eq!(config.theme.text_secondary(), tui::style::Color::Rgb(0, 255, 0));
        assert_eq!(config.theme.border_color(), tui::style::Color::Rgb(0, 0, 255));
    }

    #[test]
    fn test_config_with_partial_theme() {
        let yaml = "theme:\n    text_primary: \"#ff0000\"";
        let config: Option<Config> = serde_yaml::from_str(yaml).expect("Failed to parse config");
        assert!(config.is_some());
        
        let config = config.unwrap();
        assert_eq!(config.theme.text_primary(), tui::style::Color::Rgb(255, 0, 0));
        // Other values should fall back to defaults
        assert_eq!(config.theme.text_secondary(), tui::style::Color::Magenta);
        assert_eq!(config.theme.border_color(), tui::style::Color::Magenta);
    }

    #[test]
    fn test_empty_config() {
        let yaml = "null";
        let result = serde_yaml::from_str::<Option<Config>>(yaml);
        // Empty/null config parses as Ok(None)
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_invalid_config() {
        let yaml = "this is not valid yaml: [[[";
        let result = serde_yaml::from_str::<Option<Config>>(yaml);
        assert!(result.is_err());
    }
}
