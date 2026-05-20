use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

const DEFAULT_CONFIG_PATH: &str = "osm_pbf_processor.toml";

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct AppConfig {
    pub(crate) conversion: ConversionConfig,
    pub(crate) server: ServerConfig,
}

impl AppConfig {
    pub(crate) fn load(explicit_path: Option<&Path>) -> Result<Self> {
        let Some(path) = config_path(explicit_path) else {
            return Ok(Self::default());
        };

        let raw = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("failed to parse config {}", path.display()))
    }
}

#[derive(Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct ConversionConfig {
    pub(crate) output: Option<PathBuf>,
    pub(crate) output_glb: Option<PathBuf>,
    pub(crate) simplify: SimplifyConfig,
    pub(crate) land: LandConfig,
    pub(crate) transportation: TransportationConfig,
    pub(crate) building: BuildingConfig,
    pub(crate) water: WaterConfig,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct ServerConfig {
    pub(crate) bind: String,
    pub(crate) port: u16,
    pub(crate) backend: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: "0.0.0.0".to_string(),
            port: 8081,
            backend: "http://localhost:8080".to_string(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct SimplifyConfig {
    pub(crate) enabled: bool,
    pub(crate) tolerance: f64,
}

impl Default for SimplifyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            tolerance: 1.0,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct LandConfig {
    pub(crate) enabled: bool,
    pub(crate) layer: String,
    pub(crate) blend_layers: Vec<String>,
    pub(crate) fill_tile: bool,
    pub(crate) clip_transportation: bool,
    pub(crate) elevation: f32,
    pub(crate) xy_scale: f32,
    pub(crate) base_color: [f32; 4],
}

impl Default for LandConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            layer: "landcover".to_string(),
            blend_layers: vec!["landuse".to_string()],
            fill_tile: true,
            clip_transportation: true,
            elevation: 0.0,
            xy_scale: 1.0,
            base_color: [0.45, 0.63, 0.34, 1.0],
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct TransportationConfig {
    pub(crate) enabled: bool,
    pub(crate) layer: String,
    pub(crate) class_filter: Option<String>,
    pub(crate) exclude_class: Option<String>,
    pub(crate) line_width: f64,
    pub(crate) elevation: f32,
    pub(crate) xy_scale: f32,
    pub(crate) base_color: [f32; 4],
}

impl Default for TransportationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            layer: "transportation".to_string(),
            class_filter: None,
            exclude_class: None,
            line_width: 6.0,
            elevation: 0.02,
            xy_scale: 1.0,
            base_color: [0.34, 0.34, 0.36, 1.0],
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct BuildingConfig {
    pub(crate) enabled: bool,
    pub(crate) layer: String,
    pub(crate) default_height: f32,
    pub(crate) default_min_height: f32,
    pub(crate) height_scale: f32,
    pub(crate) xy_scale: f32,
    pub(crate) base_color: [f32; 4],
}

impl Default for BuildingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            layer: "building".to_string(),
            default_height: 10.0,
            default_min_height: 0.0,
            height_scale: 2.0,
            xy_scale: 1.0,
            base_color: [0.72, 0.70, 0.66, 1.0],
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default)]
pub(crate) struct WaterConfig {
    pub(crate) enabled: bool,
    pub(crate) layer: String,
    pub(crate) depth: f32,
    pub(crate) water_level: f32,
    pub(crate) xy_scale: f32,
    pub(crate) surface_color: [f32; 4],
    pub(crate) volume_color: [f32; 4],
}

impl Default for WaterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            layer: "water".to_string(),
            depth: 5.0,
            water_level: -0.1,
            xy_scale: 1.0,
            surface_color: [0.1, 0.35, 0.8, 0.55],
            volume_color: [0.38, 0.33, 0.26, 1.0],
        }
    }
}

fn config_path(explicit_path: Option<&Path>) -> Option<PathBuf> {
    explicit_path.map(Path::to_path_buf).or_else(|| {
        Path::new(DEFAULT_CONFIG_PATH)
            .is_file()
            .then(|| PathBuf::from(DEFAULT_CONFIG_PATH))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_typed_water_config() {
        let config: AppConfig = toml::from_str(
            r#"
            [conversion]
            output = "custom/output"
            output_glb = "custom.glb"
            [conversion.simplify]
            enabled = true
            tolerance = 3.5

            [conversion.land]
            blend_layers = ["landuse"]
            fill_tile = true
            clip_transportation = true
            elevation = 2.0
            xy_scale = 0.25
            base_color = [0.5, 0.6, 0.3, 1.0]

            [conversion.transportation]
            enabled = true
            layer = "transportation"
            class_filter = "ferry"
            exclude_class = "rail"
            line_width = 8.0
            elevation = 2.1
            xy_scale = 0.25
            base_color = [0.3, 0.3, 0.32, 1.0]

            [conversion.building]
            enabled = true
            layer = "building"
            default_height = 12.0
            default_min_height = 0.0
            height_scale = 3.0
            xy_scale = 0.25
            base_color = [0.72, 0.70, 0.66, 1.0]

            [conversion.water]
            depth = 12.5
            water_level = 3.0
            xy_scale = 0.25
            surface_color = [0.2, 0.4, 0.9, 0.5]
            volume_color = [0.38, 0.33, 0.26, 1.0]

            [server]
            bind = "127.0.0.1"
            port = 9000
            backend = "http://tiles.example.com"
            "#,
        )
        .expect("config should parse");

        assert_eq!(
            config.conversion.output,
            Some(PathBuf::from("custom/output"))
        );
        assert_eq!(
            config.conversion.output_glb,
            Some(PathBuf::from("custom.glb"))
        );
        assert!(config.conversion.simplify.enabled);
        assert_eq!(config.conversion.simplify.tolerance, 3.5);
        assert_eq!(config.conversion.land.blend_layers, vec!["landuse"]);
        assert!(config.conversion.land.fill_tile);
        assert!(config.conversion.land.clip_transportation);
        assert_eq!(config.conversion.land.elevation, 2.0);
        assert_eq!(config.conversion.land.xy_scale, 0.25);
        assert!(config.conversion.transportation.enabled);
        assert_eq!(config.conversion.transportation.layer, "transportation");
        assert_eq!(
            config.conversion.transportation.class_filter.as_deref(),
            Some("ferry")
        );
        assert_eq!(
            config.conversion.transportation.exclude_class.as_deref(),
            Some("rail")
        );
        assert_eq!(config.conversion.transportation.line_width, 8.0);
        assert_eq!(config.conversion.transportation.elevation, 2.1);
        assert!(config.conversion.building.enabled);
        assert_eq!(config.conversion.building.layer, "building");
        assert_eq!(config.conversion.building.default_height, 12.0);
        assert_eq!(config.conversion.building.height_scale, 3.0);
        assert_eq!(config.conversion.water.depth, 12.5);
        assert_eq!(config.conversion.water.water_level, 3.0);
        assert_eq!(config.conversion.water.xy_scale, 0.25);
        assert_eq!(config.conversion.water.surface_color, [0.2, 0.4, 0.9, 0.5]);
        assert_eq!(
            config.conversion.water.volume_color,
            [0.38, 0.33, 0.26, 1.0]
        );
        assert_eq!(config.server.bind, "127.0.0.1");
        assert_eq!(config.server.port, 9000);
        assert_eq!(config.server.backend, "http://tiles.example.com");
    }

    #[test]
    fn defaults_config_when_conversion_section_is_missing() {
        let config: AppConfig = toml::from_str("title = 'ignored'").expect("config should parse");

        assert_eq!(config.conversion.simplify, SimplifyConfig::default());
        assert_eq!(config.conversion.land, LandConfig::default());
        assert_eq!(config.conversion.water, WaterConfig::default());
        assert!(config.conversion.output.is_none());
        assert!(config.conversion.output_glb.is_none());
        assert_eq!(config.server, ServerConfig::default());
    }
}
