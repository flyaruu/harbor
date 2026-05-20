use std::collections::BTreeMap;

use crate::mvt;

pub(crate) fn print_report(source: &str, decoded_bytes: &[u8], report: &TileReport) {
    println!("Tile: {source}");
    println!("Decoded protobuf bytes: {}", decoded_bytes.len());
    println!("Layers: {}", report.layers.len());
    println!();

    for layer in &report.layers {
        println!(
            "{}: extent={} version={} features={} vertices={} rings={} paths={}",
            layer.name,
            layer.extent,
            layer.version,
            layer.features,
            layer.geometry.vertices,
            layer.geometry.rings,
            layer.geometry.paths
        );

        println!(
            "  geometry: points={} lines={} polygons={} unknown={}",
            layer.geometry.points,
            layer.geometry.lines,
            layer.geometry.polygons,
            layer.geometry.unknown
        );

        if !layer.top_keys.is_empty() {
            println!("  top keys: {}", format_counts(&layer.top_keys));
        }

        if !layer.numeric_values.is_empty() {
            for (key, stats) in &layer.numeric_values {
                println!(
                    "  {key}: count={} min={} max={} avg={:.2}",
                    stats.count,
                    format_number(stats.min),
                    format_number(stats.max),
                    stats.average()
                );
            }
        }

        if !layer.decode_warnings.is_empty() {
            println!("  decode warnings: {}", layer.decode_warnings.join(", "));
        }

        println!();
    }
}

fn format_counts(counts: &[(String, usize)]) -> String {
    counts
        .iter()
        .map(|(key, count)| format!("{key}={count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_number(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

#[derive(Debug)]
pub(crate) struct TileReport {
    layers: Vec<LayerReport>,
}

impl TileReport {
    pub(crate) fn from_tile(tile: &mvt::Tile) -> Self {
        let layers = tile.layers.iter().map(LayerReport::from_layer).collect();
        Self { layers }
    }
}

#[derive(Debug)]
struct LayerReport {
    name: String,
    version: u32,
    extent: u32,
    features: usize,
    geometry: GeometryStats,
    top_keys: Vec<(String, usize)>,
    numeric_values: BTreeMap<String, NumericStats>,
    decode_warnings: Vec<String>,
}

impl LayerReport {
    fn from_layer(layer: &mvt::Layer) -> Self {
        let mut geometry = GeometryStats::default();
        let mut key_counts = BTreeMap::<String, usize>::new();
        let mut numeric_values = BTreeMap::<String, NumericStats>::new();
        let mut decode_warnings = BTreeMap::<String, usize>::new();

        for feature in &layer.features {
            geometry.add_feature_type(feature.r#type());

            match decode_geometry_stats(&feature.geometry) {
                Ok(stats) => geometry += stats,
                Err(error) => {
                    *decode_warnings.entry(error).or_default() += 1;
                }
            }

            for tag in feature.tags.chunks_exact(2) {
                let key_index = tag[0] as usize;
                let value_index = tag[1] as usize;
                let Some(key) = layer.keys.get(key_index) else {
                    *decode_warnings
                        .entry("tag key index out of range".to_string())
                        .or_default() += 1;
                    continue;
                };
                let Some(value) = layer.values.get(value_index) else {
                    *decode_warnings
                        .entry("tag value index out of range".to_string())
                        .or_default() += 1;
                    continue;
                };

                *key_counts.entry(key.clone()).or_default() += 1;

                if let Some(number) = value.as_f64() {
                    numeric_values.entry(key.clone()).or_default().add(number);
                }
            }

            if feature.tags.len() % 2 != 0 {
                *decode_warnings
                    .entry("odd tag stream length".to_string())
                    .or_default() += 1;
            }
        }

        let mut top_keys: Vec<_> = key_counts.into_iter().collect();
        top_keys.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        top_keys.truncate(8);

        let decode_warnings = decode_warnings
            .into_iter()
            .map(|(warning, count)| format!("{warning} ({count})"))
            .collect();

        Self {
            name: layer.name.clone(),
            version: layer.version.unwrap_or(1),
            extent: layer.extent.unwrap_or(4096),
            features: layer.features.len(),
            geometry,
            top_keys,
            numeric_values,
            decode_warnings,
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct NumericStats {
    count: usize,
    min: f64,
    max: f64,
    sum: f64,
}

impl NumericStats {
    fn add(&mut self, value: f64) {
        if self.count == 0 {
            self.min = value;
            self.max = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }

        self.count += 1;
        self.sum += value;
    }

    fn average(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct GeometryStats {
    points: usize,
    lines: usize,
    polygons: usize,
    unknown: usize,
    vertices: usize,
    rings: usize,
    paths: usize,
}

impl GeometryStats {
    fn add_feature_type(&mut self, geometry_type: mvt::GeomType) {
        match geometry_type {
            mvt::GeomType::Point => self.points += 1,
            mvt::GeomType::LineString => self.lines += 1,
            mvt::GeomType::Polygon => self.polygons += 1,
            mvt::GeomType::Unknown => self.unknown += 1,
        }
    }
}

impl std::ops::AddAssign for GeometryStats {
    fn add_assign(&mut self, rhs: Self) {
        self.vertices += rhs.vertices;
        self.rings += rhs.rings;
        self.paths += rhs.paths;
    }
}

fn decode_geometry_stats(commands: &[u32]) -> std::result::Result<GeometryStats, String> {
    let mut cursor = 0usize;
    let mut x = 0i32;
    let mut y = 0i32;
    let mut stats = GeometryStats::default();

    while cursor < commands.len() {
        let command = commands[cursor];
        cursor += 1;

        let command_id = command & 0x7;
        let count = command >> 3;

        match command_id {
            1 => {
                stats.paths += count as usize;
                for _ in 0..count {
                    let (dx, dy) = read_point_delta(commands, &mut cursor)?;
                    x = x.wrapping_add(dx);
                    y = y.wrapping_add(dy);
                    stats.vertices += 1;
                }
            }
            2 => {
                for _ in 0..count {
                    let (dx, dy) = read_point_delta(commands, &mut cursor)?;
                    x = x.wrapping_add(dx);
                    y = y.wrapping_add(dy);
                    stats.vertices += 1;
                }
            }
            7 => {
                stats.rings += count as usize;
            }
            other => return Err(format!("unsupported geometry command {other}")),
        }
    }

    let _ = (x, y);
    Ok(stats)
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
