mod config;
mod module_bindings;
mod server;
mod tile;
mod poi;

use std::path::PathBuf;

use anyhow::Result;

use crate::config::AppConfig;

fn main() -> Result<()> {
    env_logger::init();
    let args: Vec<String> = std::env::args().collect();
    let config_path = config_path_from_args(&args)?;
    let config = AppConfig::load(config_path.as_deref())?;
    server::run(&config)
}

fn config_path_from_args(args: &[String]) -> Result<Option<PathBuf>> {
    let mut index = 1usize;
    while index < args.len() {
        if args[index] == "--config" {
            let Some(path) = args.get(index + 1) else {
                return Err(anyhow::anyhow!("missing config path"));
            };
            return Ok(Some(PathBuf::from(path)));
        }
        index += 1;
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::config_path_from_args;

    #[test]
    fn accepts_explicit_config_path() {
        let args = vec![
            "osm_pbf_processor".to_string(),
            "--config".to_string(),
            "custom.toml".to_string(),
        ];

        assert_eq!(
            config_path_from_args(&args).expect("args should parse"),
            Some(std::path::PathBuf::from("custom.toml"))
        );
    }

    #[test]
    fn ignores_unknown_flags() {
        let args = vec!["osm_pbf_processor".to_string(), "--server".to_string()];

        assert_eq!(config_path_from_args(&args).expect("args should parse"), None);
    }
}
