use std::path::PathBuf;

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct Config {
    pub server_ip: String,
    pub server_port: u16,
}

#[derive(Debug)]
pub enum ConfigError {
    NoHomeDir,
    FileNotFound(PathBuf),
    Parse(PathBuf, String),
    Serialize(String),
    Io(PathBuf, String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NoHomeDir => write!(f, "cannot determine home directory"),
            ConfigError::FileNotFound(path) => {
                write!(f, "config file not found: {}", path.display())
            }
            ConfigError::Parse(path, e) => {
                write!(f, "failed to parse config {}: {e}", path.display())
            }
            ConfigError::Serialize(e) => write!(f, "failed to serialize config: {e}"),
            ConfigError::Io(path, e) => write!(f, "IO error on {}: {e}", path.display()),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn load() -> Result<Config, ConfigError> {
        let path = config_path()?;
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                toml::from_str(&content).map_err(|e| ConfigError::Parse(path, e.to_string()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                Err(ConfigError::FileNotFound(path))
            }
            Err(e) => Err(ConfigError::Io(path, e.to_string())),
        }
    }

    pub fn create_default() -> Result<Config, ConfigError> {
        let config = Config {
            server_ip: "192.168.0.200".to_string(),
            server_port: 12345,
        };
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| ConfigError::Io(path.clone(), e.to_string()))?;
        }
        let content =
            toml::to_string(&config).map_err(|e| ConfigError::Serialize(e.to_string()))?;
        std::fs::write(&path, &content).map_err(|e| ConfigError::Io(path, e.to_string()))?;
        Ok(config)
    }

    #[cfg(test)]
    pub fn test_default() -> Config {
        Config {
            server_ip: "192.168.0.200".to_string(),
            server_port: 12345,
        }
    }
}

fn config_path() -> Result<PathBuf, ConfigError> {
    dirs::home_dir()
        .map(|h| h.join(".clipshare.toml"))
        .ok_or(ConfigError::NoHomeDir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_config() {
        let toml_str = r#"
server_ip = "192.168.1.100"
server_port = 54321
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.server_ip, "192.168.1.100");
        assert_eq!(config.server_port, 54321);
    }

    #[test]
    fn parses_default_values() {
        let config = Config::test_default();
        assert_eq!(config.server_ip, "192.168.0.200");
        assert_eq!(config.server_port, 12345);
    }

    #[test]
    fn serializes_and_deserializes() {
        let original = Config::test_default();
        let toml_str = toml::to_string(&original).unwrap();
        let restored: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(original.server_ip, restored.server_ip);
        assert_eq!(original.server_port, restored.server_port);
    }

    #[test]
    fn rejects_invalid_toml() {
        let result: Result<Config, toml::de::Error> = toml::from_str("not valid toml {{{");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_missing_fields() {
        let result: Result<Config, toml::de::Error> = toml::from_str("server_ip = \"1.2.3.4\"");
        assert!(result.is_err());
    }
}
