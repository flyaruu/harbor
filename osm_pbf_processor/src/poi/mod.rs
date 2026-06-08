use std::collections::HashMap;
use std::fs::File;
use std::path::Path;

use anyhow::{Context, Result};
use log::info;
use osmpbfreader::{OsmObj, OsmPbfReader};

use crate::config::PoiConfig;
use crate::module_bindings::{GeoPoint, StreetPoi, TownPoi};

const TOWN_PLACE_VALUES: &[&str] = &["city", "town", "village", "hamlet"];
const STREET_HIGHWAY_VALUES: &[&str] = &[
    "motorway",
    "trunk",
    "primary",
    "secondary",
    "tertiary",
    "unclassified",
    "residential",
    "living_street",
    "service",
    "road",
    "motorway_link",
    "trunk_link",
    "primary_link",
    "secondary_link",
    "tertiary_link",
];
const MAX_TOWN_ASSIGNMENT_DISTANCE_METERS: f64 = 25_000.0;

pub(crate) fn import_towns(config: &PoiConfig) -> Result<Vec<TownPoi>> {
    load_towns(&config.osm_pbf_path)
}

pub(crate) fn import_streets(config: &PoiConfig, towns: &[TownPoi]) -> Result<Vec<StreetPoi>> {
    load_streets(&config.osm_pbf_path, &towns)
}

fn load_towns(path: &Path) -> Result<Vec<TownPoi>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = OsmPbfReader::new(file);
    info!("loading towns from {}", path.display());
    let mut towns = Vec::new();

    for object in reader.iter() {
        let OsmObj::Node(node) = object? else {
            continue;
        };

        let Some(place) = node.tags.get("place") else {
            continue;
        };
        if !TOWN_PLACE_VALUES.contains(&place.as_str()) {
            continue;
        }

        let Some(name) = node.tags.get("name") else {
            continue;
        };

        info!("found town: {} (place={}, osm_id={})", name, place, node.id.0);
        towns.push(TownPoi {
            id: 0,
            osm_id: node.id.0 as i64,
            name: name.to_string(),
            place: place.to_string(),
            location: GeoPoint {
                lon: node.lon(),
                lat: node.lat(),
            },
        });
    }

    Ok(towns)
}

fn load_streets(path: &Path, towns: &[TownPoi]) -> Result<Vec<StreetPoi>> {
    let town_index = TownIndex::new(towns);
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut reader = OsmPbfReader::new(file);
    let mut node_cache: HashMap<i64, GeoPoint> = HashMap::new();
    let mut streets = Vec::new();

    for object in reader.iter() {
        match object? {
            OsmObj::Node(node) => {
                node_cache.insert(
                    node.id.0 as i64,
                    GeoPoint {
                        lon: node.lon(),
                        lat: node.lat(),
                    },
                );
            }
            OsmObj::Way(way) => {
                let Some(highway) = way.tags.get("highway") else {
                    continue;
                };
                if !STREET_HIGHWAY_VALUES.contains(&highway.as_str()) {
                    continue;
                }

                let Some(name) = way.tags.get("name").or_else(|| way.tags.get("ref")) else {
                    continue;
                };

                let path_points = way
                    .nodes
                    .iter()
                    .filter_map(|node_id| node_cache.get(&(node_id.0 as i64)).cloned())
                    .collect::<Vec<_>>();

                if path_points.len() < 2 {
                    continue;
                }

                let center = centroid(&path_points);
                let matched_town = town_index.nearest(&center);

                streets.push(StreetPoi {
                    id: 0,
                    osm_id: way.id.0 as i64,
                    name: name.to_string(),
                    highway: highway.to_string(),
                    path: path_points,
                    town_osm_id: matched_town.map(|town| town.osm_id),
                    town_name: matched_town.map(|town| town.name.clone()),
                    town_distance_m: matched_town.map(|town| distance_m(&center, &town.location)),
                    center,
                });
            }
            OsmObj::Relation(_) => {}
        }
    }

    Ok(streets)
}

fn centroid(points: &[GeoPoint]) -> GeoPoint {
    let mut lon_sum = 0.0;
    let mut lat_sum = 0.0;
    for point in points {
        lon_sum += point.lon;
        lat_sum += point.lat;
    }

    let len = points.len() as f64;
    GeoPoint {
        lon: lon_sum / len,
        lat: lat_sum / len,
    }
}

fn distance_m(a: &GeoPoint, b: &GeoPoint) -> f64 {
    let radius_m = 6_371_000.0;
    let d_lat = (b.lat - a.lat).to_radians();
    let d_lon = (b.lon - a.lon).to_radians();
    let lat1 = a.lat.to_radians();
    let lat2 = b.lat.to_radians();

    let sin_lat = (d_lat / 2.0).sin();
    let sin_lon = (d_lon / 2.0).sin();
    let h = sin_lat * sin_lat + lat1.cos() * lat2.cos() * sin_lon * sin_lon;
    2.0 * radius_m * h.sqrt().asin()
}

struct TownIndex<'a> {
    towns: &'a [TownPoi],
}

impl<'a> TownIndex<'a> {
    fn new(towns: &'a [TownPoi]) -> Self {
        Self { towns }
    }

    fn nearest(&self, point: &GeoPoint) -> Option<&'a TownPoi> {
        let nearest = self.towns.iter().min_by(|left, right| {
            let left_distance = distance_m(point, &left.location);
            let right_distance = distance_m(point, &right.location);
            left_distance
                .partial_cmp(&right_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        })?;

        if distance_m(point, &nearest.location) <= MAX_TOWN_ASSIGNMENT_DISTANCE_METERS {
            Some(nearest)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centroid_averages_points() {
        let centroid = centroid(&[
            GeoPoint { lon: 0.0, lat: 0.0 },
            GeoPoint { lon: 2.0, lat: 0.0 },
            GeoPoint { lon: 2.0, lat: 2.0 },
            GeoPoint { lon: 0.0, lat: 2.0 },
        ]);

        assert_eq!(centroid, GeoPoint { lon: 1.0, lat: 1.0 });
    }

    #[test]
    fn town_index_picks_nearest_town() {
        let towns = vec![
            TownPoi {
                id: 0,
                osm_id: 1,
                name: "Alpha".to_string(),
                place: "town".to_string(),
                location: GeoPoint { lon: 0.0, lat: 0.0 },
            },
            TownPoi {
                id: 0,
                osm_id: 2,
                name: "Beta".to_string(),
                place: "town".to_string(),
                location: GeoPoint { lon: 0.2, lat: 0.0 },
            },
        ];

        let town = TownIndex::new(&towns)
            .nearest(&GeoPoint { lon: 0.18, lat: 0.0 })
            .expect("town should exist");

        assert_eq!(town.name, "Beta");
    }
}
