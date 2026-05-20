use anyhow::Result;
use geo::{Buffer, Coord, LineString, MultiPolygon, unary_union};
use std::time::{Duration, Instant};

use crate::clip::clip_to_tile_bounds;
use crate::config::{SimplifyConfig, TransportationConfig};
use crate::mvt;
use crate::polygon_decode::decode_feature_polygons;
use crate::simplify::simplify_polygons;

#[derive(Debug)]
pub(crate) struct TransportationGeometry {
    pub(crate) extent: u32,
    pub(crate) polygons: MultiPolygon<f64>,
    pub(crate) timing: TransportationTiming,
    pub(crate) feature_counts: TransportationFeatureCounts,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TransportationTiming {
    pub(crate) decode_buffer_merge: Duration,
    pub(crate) clip: Duration,
    pub(crate) simplify: Duration,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TransportationFeatureCounts {
    pub(crate) polygons: usize,
    pub(crate) line_strings: usize,
    pub(crate) skipped: usize,
}

pub(crate) fn extract_transportation_geometry(
    tile: &mvt::Tile,
    config: &TransportationConfig,
    simplify: &SimplifyConfig,
) -> Result<Option<TransportationGeometry>> {
    let Some(layer) = tile.layers.iter().find(|layer| layer.name == config.layer) else {
        return Ok(None);
    };

    let extent = layer.extent.unwrap_or(4096);
    let mut geometries = Vec::new();
    let decode_buffer_merge_start = Instant::now();
    let mut feature_counts = TransportationFeatureCounts::default();

    for feature in &layer.features {
        if !matches_class_filter(
            feature,
            layer,
            config.class_filter.as_deref(),
            config.exclude_class.as_deref(),
        ) {
            feature_counts.skipped += 1;
            continue;
        }

        let polygons = match feature.r#type() {
            mvt::GeomType::Polygon => {
                feature_counts.polygons += 1;
                decode_feature_polygons(&feature.geometry).map_err(|error| {
                    anyhow::anyhow!(
                        "failed to decode transportation polygon feature {:?}: {error}",
                        feature.id
                    )
                })?
            }
            mvt::GeomType::LineString => {
                feature_counts.line_strings += 1;
                buffer_transportation_lines(&feature.geometry, config.line_width).map_err(
                    |error| {
                        anyhow::anyhow!(
                            "failed to decode transportation line feature {:?}: {error}",
                            feature.id
                        )
                    },
                )?
            }
            _ => {
                feature_counts.skipped += 1;
                continue;
            }
        };

        if polygons.0.is_empty() {
            continue;
        }

        geometries.push(polygons);
    }
    let decode_buffer_merge = decode_buffer_merge_start.elapsed();

    if geometries.is_empty() {
        return Ok(None);
    }
    let merged = unary_union(geometries.iter());
    let clip_start = Instant::now();
    let polygons = clip_to_tile_bounds(&merged, extent);
    let clip = clip_start.elapsed();
    let simplify_start = Instant::now();
    let (polygons, simplify_stats) = simplify_polygons(&polygons, extent, simplify);
    let simplify_duration = simplify_start.elapsed();

    if simplify.enabled {
        println!(
            "Transportation simplification: vertices {} -> {}",
            simplify_stats.before_vertices, simplify_stats.after_vertices
        );
    }

    if polygons.0.is_empty() {
        Ok(None)
    } else {
        Ok(Some(TransportationGeometry {
            extent,
            polygons,
            timing: TransportationTiming {
                decode_buffer_merge,
                clip,
                simplify: simplify_duration,
            },
            feature_counts,
        }))
    }
}

fn matches_class_filter(
    feature: &mvt::Feature,
    layer: &mvt::Layer,
    class_filter: Option<&str>,
    exclude_class: Option<&str>,
) -> bool {
    let class_value = feature_tag_value(feature, layer, "class");

    if let Some(class_filter) = class_filter
        && class_value != Some(class_filter)
    {
        return false;
    }

    if let Some(exclude_class) = exclude_class
        && class_value == Some(exclude_class)
    {
        return false;
    }

    true
}

fn feature_tag_value<'a>(
    feature: &mvt::Feature,
    layer: &'a mvt::Layer,
    key_name: &str,
) -> Option<&'a str> {
    for tag in feature.tags.chunks_exact(2) {
        let key_index = tag[0] as usize;
        let value_index = tag[1] as usize;
        let key = layer.keys.get(key_index)?;
        if key != key_name {
            continue;
        }

        return layer.values.get(value_index)?.as_str();
    }

    None
}

fn buffer_transportation_lines(
    commands: &[u32],
    line_width: f64,
) -> std::result::Result<MultiPolygon<f64>, String> {
    let lines = decode_feature_line_strings(commands)?;
    let distance = line_width * 0.5;
    let mut buffered_lines = Vec::new();

    for line in lines {
        if line.0.len() < 2 {
            continue;
        }

        buffered_lines.push(line.buffer(distance));
    }

    Ok(if buffered_lines.is_empty() {
        MultiPolygon(vec![])
    } else {
        unary_union(buffered_lines.iter())
    })
}

fn decode_feature_line_strings(
    commands: &[u32],
) -> std::result::Result<Vec<LineString<f64>>, String> {
    let mut cursor = 0usize;
    let mut x = 0i32;
    let mut y = 0i32;
    let mut current_path: Vec<Coord<f64>> = Vec::new();
    let mut paths = Vec::new();

    while cursor < commands.len() {
        let command = commands[cursor];
        cursor += 1;

        let command_id = command & 0x7;
        let count = command >> 3;

        match command_id {
            1 => {
                if !current_path.is_empty() {
                    if current_path.len() >= 2 {
                        paths.push(LineString::new(current_path));
                    }
                    current_path = Vec::new();
                }

                for _ in 0..count {
                    let (dx, dy) = read_point_delta(commands, &mut cursor)?;
                    x = x.wrapping_add(dx);
                    y = y.wrapping_add(dy);
                    current_path.push(Coord {
                        x: x as f64,
                        y: y as f64,
                    });
                }
            }
            2 => {
                if current_path.is_empty() {
                    return Err("LineTo encountered before line MoveTo".to_string());
                }

                for _ in 0..count {
                    let (dx, dy) = read_point_delta(commands, &mut cursor)?;
                    x = x.wrapping_add(dx);
                    y = y.wrapping_add(dy);
                    current_path.push(Coord {
                        x: x as f64,
                        y: y as f64,
                    });
                }
            }
            7 => {
                // Transportation line features should not use ClosePath, but ignore if present.
            }
            other => return Err(format!("unsupported geometry command {other}")),
        }
    }

    if current_path.len() >= 2 {
        paths.push(LineString::new(current_path));
    }

    Ok(paths)
}

fn read_point_delta(
    commands: &[u32],
    cursor: &mut usize,
) -> std::result::Result<(i32, i32), String> {
    if *cursor + 1 >= commands.len() {
        return Err("truncated point delta".to_string());
    }

    let dx = zigzag_decode(commands[*cursor]);
    let dy = zigzag_decode(commands[*cursor + 1]);
    *cursor += 2;
    Ok((dx, dy))
}

fn zigzag_decode(value: u32) -> i32 {
    ((value >> 1) as i32) ^ (-((value & 1) as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffers_line_geometry_into_polygons() {
        let commands = vec![9, 0, 0, 18, 20, 0, 0, 20];
        let polygons = buffer_transportation_lines(&commands, 4.0).expect("buffer should succeed");

        assert!(!polygons.0.is_empty());
    }

    #[test]
    fn missing_transportation_layer_is_not_an_error() {
        let tile = mvt::Tile { layers: vec![] };

        let transportation = extract_transportation_geometry(
            &tile,
            &TransportationConfig::default(),
            &SimplifyConfig::default(),
        )
        .expect("missing layer should not fail");

        assert!(transportation.is_none());
    }

    #[test]
    fn filters_transportation_by_class() {
        let layer = mvt::Layer {
            name: "transportation".to_string(),
            features: vec![
                mvt::Feature {
                    id: Some(1),
                    tags: vec![0, 0],
                    r#type: Some(mvt::GeomType::LineString as i32),
                    geometry: vec![9, 0, 0, 10, 20, 0],
                },
                mvt::Feature {
                    id: Some(2),
                    tags: vec![0, 1],
                    r#type: Some(mvt::GeomType::LineString as i32),
                    geometry: vec![9, 0, 0, 10, 0, 20],
                },
            ],
            keys: vec!["class".to_string()],
            values: vec![
                mvt::Value {
                    string_value: Some("ferry".to_string()),
                    float_value: None,
                    double_value: None,
                    int_value: None,
                    uint_value: None,
                    sint_value: None,
                    bool_value: None,
                },
                mvt::Value {
                    string_value: Some("road".to_string()),
                    float_value: None,
                    double_value: None,
                    int_value: None,
                    uint_value: None,
                    sint_value: None,
                    bool_value: None,
                },
            ],
            extent: Some(4096),
            version: Some(2),
        };
        let tile = mvt::Tile {
            layers: vec![layer],
        };
        let mut config = TransportationConfig::default();
        config.class_filter = Some("ferry".to_string());

        let transportation =
            extract_transportation_geometry(&tile, &config, &SimplifyConfig::default())
                .expect("filtered transportation should decode")
                .expect("ferry feature should remain");

        assert_eq!(transportation.feature_counts.line_strings, 1);
        assert_eq!(transportation.feature_counts.skipped, 1);
    }

    #[test]
    fn excludes_transportation_class() {
        let layer = mvt::Layer {
            name: "transportation".to_string(),
            features: vec![
                mvt::Feature {
                    id: Some(1),
                    tags: vec![0, 0],
                    r#type: Some(mvt::GeomType::LineString as i32),
                    geometry: vec![9, 0, 0, 10, 20, 0],
                },
                mvt::Feature {
                    id: Some(2),
                    tags: vec![0, 1],
                    r#type: Some(mvt::GeomType::LineString as i32),
                    geometry: vec![9, 0, 0, 10, 0, 20],
                },
            ],
            keys: vec!["class".to_string()],
            values: vec![
                mvt::Value {
                    string_value: Some("ferry".to_string()),
                    float_value: None,
                    double_value: None,
                    int_value: None,
                    uint_value: None,
                    sint_value: None,
                    bool_value: None,
                },
                mvt::Value {
                    string_value: Some("road".to_string()),
                    float_value: None,
                    double_value: None,
                    int_value: None,
                    uint_value: None,
                    sint_value: None,
                    bool_value: None,
                },
            ],
            extent: Some(4096),
            version: Some(2),
        };
        let tile = mvt::Tile {
            layers: vec![layer],
        };
        let mut config = TransportationConfig::default();
        config.exclude_class = Some("ferry".to_string());

        let transportation =
            extract_transportation_geometry(&tile, &config, &SimplifyConfig::default())
                .expect("filtered transportation should decode")
                .expect("road feature should remain");

        assert_eq!(transportation.feature_counts.line_strings, 1);
        assert_eq!(transportation.feature_counts.skipped, 1);
    }
}
