use std::ops::RangeInclusive;
use std::path::PathBuf;

use anyhow::{Result, bail};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TileRequest {
    pub(crate) source_label: String,
    pub(crate) output_path: PathBuf,
    pub(crate) fetch_url: Option<String>,
    pub(crate) file_path: Option<PathBuf>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InputSource {
    File(PathBuf),
    Server,
    Url {
        base_url: String,
        zoom: u8,
        x_range: RangeInclusive<u32>,
        y_range: RangeInclusive<u32>,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CliArgs {
    pub(crate) input: InputSource,
    pub(crate) config_path: Option<PathBuf>,
}

impl CliArgs {
    pub(crate) fn parse(args: &[String]) -> Result<Option<Self>> {
        match args {
            [program] => {
                print_usage(program);
                Ok(None)
            }
            [program, rest @ ..] => {
                let mut config_path = None;
                let mut base_url = None;
                let mut server_mode = false;
                let mut zoom = 14u8;
                let mut positionals = Vec::new();
                let mut index = 0usize;

                while index < rest.len() {
                    match rest[index].as_str() {
                        "--config" => {
                            let Some(value) = rest.get(index + 1) else {
                                print_usage(program);
                                bail!("missing config path")
                            };
                            config_path = Some(PathBuf::from(value));
                            index += 2;
                        }
                        "--url" => {
                            let Some(value) = rest.get(index + 1) else {
                                print_usage(program);
                                bail!("missing url value")
                            };
                            base_url = Some(value.clone());
                            index += 2;
                        }
                        "--server" => {
                            server_mode = true;
                            index += 1;
                        }
                        "--zoom" => {
                            let Some(value) = rest.get(index + 1) else {
                                print_usage(program);
                                bail!("missing zoom value")
                            };
                            zoom = value
                                .parse()
                                .map_err(|_| anyhow::anyhow!("invalid zoom value"))?;
                            index += 2;
                        }
                        flag if flag.starts_with("--") => {
                            print_usage(program);
                            bail!("unknown flag {flag}")
                        }
                        value => {
                            positionals.push(value.to_string());
                            index += 1;
                        }
                    }
                }

                let input = if server_mode {
                    if base_url.is_some() || !positionals.is_empty() {
                        print_usage(program);
                        bail!("server mode does not accept --url or tile coordinates")
                    }

                    InputSource::Server
                } else if let Some(base_url) = base_url {
                    if positionals.len() != 2 {
                        print_usage(program);
                        bail!("url mode expects x and y coordinates")
                    }

                    InputSource::Url {
                        base_url,
                        zoom,
                        x_range: parse_coordinate_range(&positionals[0], "x")?,
                        y_range: parse_coordinate_range(&positionals[1], "y")?,
                    }
                } else {
                    if positionals.len() != 1 {
                        print_usage(program);
                        bail!("file mode expects a single tile path")
                    }

                    InputSource::File(PathBuf::from(&positionals[0]))
                };

                Ok(Some(Self { input, config_path }))
            }
            [] => unreachable!(),
        }
    }

    pub(crate) fn tile_requests(&self) -> Vec<TileRequest> {
        match &self.input {
            InputSource::File(path) => vec![TileRequest {
                source_label: path.display().to_string(),
                output_path: path.with_extension("glb"),
                fetch_url: None,
                file_path: Some(path.clone()),
            }],
            InputSource::Server => Vec::new(),
            InputSource::Url {
                base_url,
                zoom,
                x_range,
                y_range,
            } => {
                let mut requests = Vec::new();
                for x in x_range.clone() {
                    for y in y_range.clone() {
                        requests.push(TileRequest {
                            source_label: build_tile_url(base_url, *zoom, x, y),
                            output_path: tile_output_path(*zoom, x, y),
                            fetch_url: Some(build_tile_url(base_url, *zoom, x, y)),
                            file_path: None,
                        });
                    }
                }
                requests
            }
        }
    }
}

fn tile_output_path(zoom: u8, x: u32, y: u32) -> PathBuf {
    PathBuf::from(zoom.to_string()).join(format!("{x}_{y}.glb"))
}

fn print_usage(program: &str) {
    eprintln!("Usage: {program} [--config <config.toml>] <tile.pbf>");
    eprintln!(
        "   or: {program} [--config <config.toml>] --url <server-url> <x|x0-x1> <y|y0-y1> [--zoom <z>]"
    );
    eprintln!("   or: {program} [--config <config.toml>] --server");
    eprintln!("Prints a decoded Mapbox Vector Tile report and exports water geometry to GLB.");
}

fn parse_coordinate_range(value: &str, axis: &str) -> Result<RangeInclusive<u32>> {
    if let Some((start, end)) = value.split_once('-') {
        let start = start
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("invalid {axis} range start"))?;
        let end = end
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("invalid {axis} range end"))?;
        if start > end {
            bail!("invalid {axis} range: start must be <= end")
        }
        Ok(start..=end)
    } else {
        let value = value
            .parse::<u32>()
            .map_err(|_| anyhow::anyhow!("invalid {axis} coordinate"))?;
        Ok(value..=value)
    }
}

pub(crate) fn build_tile_url(base_url: &str, zoom: u8, x: u32, y: u32) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.contains("{z}") || trimmed.contains("{x}") || trimmed.contains("{y}") {
        return trimmed
            .replace("{z}", &zoom.to_string())
            .replace("{x}", &x.to_string())
            .replace("{y}", &y.to_string());
    }

    if let Some(prefix) = trimmed.rsplit_once("/v3/").map(|(prefix, _)| prefix) {
        return format!("{prefix}/v3/{zoom}/{x}/{y}.pbf");
    }

    if trimmed.ends_with("/v3") {
        format!("{trimmed}/{zoom}/{x}/{y}.pbf")
    } else {
        format!("{trimmed}/v3/{zoom}/{x}/{y}.pbf")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cli_with_explicit_config() {
        let args = vec![
            "osm_pbf_processor".to_string(),
            "--config".to_string(),
            "custom.toml".to_string(),
            "5421.pbf".to_string(),
        ];

        let cli = CliArgs::parse(&args)
            .expect("cli should parse")
            .expect("cli args should be present");

        assert_eq!(cli.input, InputSource::File(PathBuf::from("5421.pbf")));
        assert_eq!(cli.config_path, Some(PathBuf::from("custom.toml")));
    }

    #[test]
    fn parses_cli_url_mode_with_default_zoom() {
        let args = vec![
            "osm_pbf_processor".to_string(),
            "--url".to_string(),
            "http://localhost:8080/data".to_string(),
            "8396".to_string(),
            "5421".to_string(),
        ];

        let cli = CliArgs::parse(&args)
            .expect("cli should parse")
            .expect("cli args should be present");

        assert_eq!(
            cli.input,
            InputSource::Url {
                base_url: "http://localhost:8080/data".to_string(),
                zoom: 14,
                x_range: 8396..=8396,
                y_range: 5421..=5421,
            }
        );
    }

    #[test]
    fn parses_cli_server_mode() {
        let args = vec!["osm_pbf_processor".to_string(), "--server".to_string()];

        let cli = CliArgs::parse(&args)
            .expect("cli should parse")
            .expect("cli args should be present");

        assert_eq!(cli.input, InputSource::Server);
    }

    #[test]
    fn rejects_server_mode_with_url() {
        let args = vec![
            "osm_pbf_processor".to_string(),
            "--server".to_string(),
            "--url".to_string(),
            "http://localhost:8080/data".to_string(),
        ];

        let error = CliArgs::parse(&args).expect_err("cli should reject mixed server/url mode");

        assert!(
            error
                .to_string()
                .contains("server mode does not accept --url or tile coordinates")
        );
    }

    #[test]
    fn rejects_server_mode_with_positionals() {
        let args = vec![
            "osm_pbf_processor".to_string(),
            "--server".to_string(),
            "8396".to_string(),
            "5421".to_string(),
        ];

        let error =
            CliArgs::parse(&args).expect_err("cli should reject positionals in server mode");

        assert!(
            error
                .to_string()
                .contains("server mode does not accept --url or tile coordinates")
        );
    }

    #[test]
    fn parses_cli_url_mode_with_ranges() {
        let args = vec![
            "osm_pbf_processor".to_string(),
            "--url".to_string(),
            "http://localhost:8080/data".to_string(),
            "8396-8398".to_string(),
            "5421-5422".to_string(),
        ];

        let cli = CliArgs::parse(&args)
            .expect("cli should parse")
            .expect("cli args should be present");

        assert_eq!(
            cli.input,
            InputSource::Url {
                base_url: "http://localhost:8080/data".to_string(),
                zoom: 14,
                x_range: 8396..=8398,
                y_range: 5421..=5422,
            }
        );
        assert_eq!(cli.tile_requests().len(), 6);
    }

    #[test]
    fn url_mode_outputs_tiles_in_zoom_folder() {
        let args = vec![
            "osm_pbf_processor".to_string(),
            "--url".to_string(),
            "http://localhost:8080/data".to_string(),
            "8396".to_string(),
            "5421".to_string(),
            "--zoom".to_string(),
            "15".to_string(),
        ];

        let cli = CliArgs::parse(&args)
            .expect("cli should parse")
            .expect("cli args should be present");

        assert_eq!(
            cli.tile_requests()[0].output_path,
            tile_output_path(15, 8396, 5421)
        );
    }

    #[test]
    fn builds_tile_url_from_sample_url() {
        let url = build_tile_url("http://localhost:8080/data/v3/14/8396/5421.pbf", 15, 1, 2);

        assert_eq!(url, "http://localhost:8080/data/v3/15/1/2.pbf");
    }

    #[test]
    fn no_args_prints_usage_without_error() {
        let args = vec!["osm_pbf_processor".to_string()];

        let cli = CliArgs::parse(&args).expect("usage should not error");

        assert!(cli.is_none());
    }
}
