mod building;
mod cli;
mod clip;
mod config;
mod gltf_export;
mod land;
mod mesh;
mod mvt;
mod polygon_decode;
mod report;
mod simplify;
mod tile_io;
mod transportation;
mod water;

use anyhow::{Context, Result};
use building::extract_building_geometry;
use cli::CliArgs;
use config::AppConfig;
use gltf_export::{SceneMesh, write_glb};
use indicatif::{ProgressBar, ProgressStyle};
use land::extract_land_geometry;
use mesh::{build_building_meshes, build_land_mesh, build_transportation_mesh, build_water_meshes};
use prost::Message as ProstMessage;
use report::{TileReport, print_report};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tile_io::{fetch_tile_bytes, read_tile_bytes};
use transportation::extract_transportation_geometry;
use water::extract_water_geometry;

#[derive(Default)]
struct ConversionBreakdown {
    lines: Vec<String>,
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let Some(cli) = CliArgs::parse(&args)? else {
        return Ok(());
    };

    let config = AppConfig::load(cli.config_path.as_deref())?;
    let requests = cli.tile_requests();
    if requests.len() > 1 && config.conversion.output_glb.is_some() {
        anyhow::bail!("output_glb cannot be used with URL x/y ranges")
    }
    let output_root = config
        .conversion
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from("output"));

    let progress = progress_bar(requests.len());

    for (index, request) in requests.iter().enumerate() {
        if let Some(progress) = progress.as_ref() {
            progress.set_message(format!(
                "tile {}/{}: {}",
                index + 1,
                requests.len(),
                request.source_label
            ));
        }

        let tile_start = Instant::now();
        let load_start = Instant::now();
        let bytes = if let Some(path) = &request.file_path {
            read_tile_bytes(path)?
        } else if let Some(url) = &request.fetch_url {
            fetch_tile_bytes(url)?
        } else {
            anyhow::bail!("tile request missing input source")
        };
        let load_elapsed = load_start.elapsed();

        let decode_start = Instant::now();
        let tile = <mvt::Tile as ProstMessage>::decode(bytes.as_slice())
            .context("failed to decode vector tile protobuf")?;
        let decode_elapsed = decode_start.elapsed();
        let report = TileReport::from_tile(&tile);

        if let Some(progress) = progress.as_ref() {
            progress.suspend(|| print_report(&request.source_label, &bytes, &report));
        } else {
            print_report(&request.source_label, &bytes, &report);
        }

        let mut scene_meshes: Vec<(String, _, _)> = Vec::new();
        let conversion_start = Instant::now();
        let mut conversion_breakdown = ConversionBreakdown::default();

        let water_extract_start = Instant::now();
        let water = if config.conversion.water.enabled {
            extract_water_geometry(&tile, &config.conversion.water, &config.conversion.simplify)?
        } else {
            None
        };
        let water_extract_elapsed = water_extract_start.elapsed();
        if let Some(water) = water.as_ref() {
            conversion_breakdown.lines.push(format!(
                "water extract={} decode+merge={} clip={} simplify={}",
                format_duration(water_extract_elapsed),
                format_duration(water.timing.decode_merge),
                format_duration(water.timing.clip),
                format_duration(water.timing.simplify),
            ));
        }

        let transportation_extract_start = Instant::now();
        let transportation = if config.conversion.transportation.enabled {
            extract_transportation_geometry(
                &tile,
                &config.conversion.transportation,
                &config.conversion.simplify,
            )?
        } else {
            None
        };
        let transportation_extract_elapsed = transportation_extract_start.elapsed();
        if let Some(transportation) = transportation.as_ref() {
            conversion_breakdown.lines.push(format!(
                "transportation extract={} decode+buffer+merge={} clip={} simplify={} features: polygons={} lines={} skipped={}",
                format_duration(transportation_extract_elapsed),
                format_duration(transportation.timing.decode_buffer_merge),
                format_duration(transportation.timing.clip),
                format_duration(transportation.timing.simplify),
                transportation.feature_counts.polygons,
                transportation.feature_counts.line_strings,
                transportation.feature_counts.skipped,
            ));
        }

        if config.conversion.land.enabled {
            let land_extract_start = Instant::now();
            if let Some(land) = extract_land_geometry(
                &tile,
                &config.conversion.land,
                &config.conversion.simplify,
                water.as_ref().map(|water| &water.polygons),
                transportation
                    .as_ref()
                    .map(|transportation| &transportation.polygons),
            )? {
                let land_extract_elapsed = land_extract_start.elapsed();
                conversion_breakdown.lines.push(format!(
                    "land extract={} source={} fill_holes={} subtract_water={} subtract_transportation={} clip={} simplify={}",
                    format_duration(land_extract_elapsed),
                    format_duration(land.timing.source),
                    format_duration(land.timing.fill_holes),
                    format_duration(land.timing.subtract_water),
                    format_duration(land.timing.subtract_transportation),
                    format_duration(land.timing.clip),
                    format_duration(land.timing.simplify),
                ));
                let land_mesh_start = Instant::now();
                let mesh = build_land_mesh(&land, &config.conversion.land)?;
                conversion_breakdown.lines.push(format!(
                    "land mesh={}",
                    format_duration(land_mesh_start.elapsed()),
                ));
                scene_meshes.push((
                    "ground".to_string(),
                    mesh,
                    config.conversion.land.base_color,
                ));
            }
        }

        if let Some(transportation) = transportation.as_ref() {
            let transportation_mesh_start = Instant::now();
            let mesh =
                build_transportation_mesh(transportation, &config.conversion.transportation)?;
            conversion_breakdown.lines.push(format!(
                "transportation mesh={}",
                format_duration(transportation_mesh_start.elapsed()),
            ));
            scene_meshes.push((
                config.conversion.transportation.layer.clone(),
                mesh,
                config.conversion.transportation.base_color,
            ));
        }

        if config.conversion.building.enabled {
            let building_extract_start = Instant::now();
            if let Some(buildings) = extract_building_geometry(
                &tile,
                &config.conversion.building,
                &config.conversion.simplify,
            )? {
                let building_extract_elapsed = building_extract_start.elapsed();
                conversion_breakdown.lines.push(format!(
                    "building extract={} decode={} clip={} simplify={} heights={}",
                    format_duration(building_extract_elapsed),
                    format_duration(buildings.timing.decode),
                    format_duration(buildings.timing.clip),
                    format_duration(buildings.timing.simplify),
                    format_duration(buildings.timing.heights),
                ));
                let building_mesh_start = Instant::now();
                let meshes = build_building_meshes(&buildings, &config.conversion.building)?;
                conversion_breakdown.lines.push(format!(
                    "building mesh={}",
                    format_duration(building_mesh_start.elapsed()),
                ));
                if !meshes.roof.is_empty() {
                    scene_meshes.push((
                        "building_roof".to_string(),
                        meshes.roof,
                        config.conversion.building.base_color,
                    ));
                }
                if !meshes.window.is_empty() {
                    scene_meshes.push((
                        "building_wall".to_string(),
                        meshes.window,
                        config.conversion.building.base_color,
                    ));
                }
            }
        }

        if let Some(water) = water.as_ref() {
            let water_mesh_start = Instant::now();
            let meshes = build_water_meshes(
                water,
                &config.conversion.water,
                config.conversion.land.elevation,
            )?;
            conversion_breakdown.lines.push(format!(
                "water mesh={}",
                format_duration(water_mesh_start.elapsed()),
            ));
            if !meshes.surface.is_empty() {
                scene_meshes.push((
                    "water_surface".to_string(),
                    meshes.surface,
                    config.conversion.water.surface_color,
                ));
            }
            if !meshes.bottom.is_empty() {
                scene_meshes.push((
                    "water_bottom".to_string(),
                    meshes.bottom,
                    config.conversion.water.volume_color,
                ));
            }
            if !meshes.side.is_empty() {
                scene_meshes.push((
                    "water_side".to_string(),
                    meshes.side,
                    config.conversion.water.volume_color,
                ));
            }
        }
        let conversion_elapsed = conversion_start.elapsed();

        let export_start = Instant::now();
        if !scene_meshes.is_empty() {
            let output_path = output_path_for_request(
                &request.output_path,
                config.conversion.output_glb.as_deref(),
                &output_root,
            );
            let export_meshes: Vec<_> = scene_meshes
                .iter()
                .map(|(name, mesh, base_color)| SceneMesh {
                    mesh,
                    material_tag: name,
                    base_color: *base_color,
                })
                .collect();
            write_glb(&output_path, &export_meshes)?;
            if let Some(progress) = progress.as_ref() {
                progress.suspend(|| println!("Wrote scene GLB: {}", output_path.display()));
            } else {
                println!("Wrote scene GLB: {}", output_path.display());
            }
        }
        let export_elapsed = export_start.elapsed();

        print_timing(
            progress.as_ref(),
            &request.source_label,
            load_elapsed,
            decode_elapsed,
            conversion_elapsed,
            export_elapsed,
            tile_start.elapsed(),
            &conversion_breakdown,
        );

        if let Some(progress) = progress.as_ref() {
            progress.inc(1);
        }
    }

    if let Some(progress) = progress {
        progress.finish_with_message("Finished rendering tiles");
    }

    Ok(())
}

fn output_path_for_request(
    request_output_path: &Path,
    output_glb: Option<&Path>,
    output_root: &Path,
) -> PathBuf {
    output_glb
        .map(Path::to_path_buf)
        .unwrap_or_else(|| output_root.join(request_output_path))
}

fn progress_bar(total_tiles: usize) -> Option<ProgressBar> {
    if total_tiles <= 1 {
        return None;
    }

    let progress = ProgressBar::new(total_tiles as u64);
    let style =
        ProgressStyle::with_template("[{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
            .expect("progress bar template should be valid")
            .progress_chars("=> ");
    progress.set_style(style);
    Some(progress)
}

fn print_timing(
    progress: Option<&ProgressBar>,
    source_label: &str,
    load_elapsed: Duration,
    decode_elapsed: Duration,
    conversion_elapsed: Duration,
    export_elapsed: Duration,
    total_elapsed: Duration,
    conversion_breakdown: &ConversionBreakdown,
) {
    let print = || {
        println!(
            "Timing for {}: load={} decode={} convert={} export={} total={}",
            source_label,
            format_duration(load_elapsed),
            format_duration(decode_elapsed),
            format_duration(conversion_elapsed),
            format_duration(export_elapsed),
            format_duration(total_elapsed),
        );
        for line in &conversion_breakdown.lines {
            println!("  {line}");
        }
    };

    if let Some(progress) = progress {
        progress.suspend(print);
    } else {
        print();
    }
}

fn format_duration(duration: Duration) -> String {
    format!("{:.1}ms", duration.as_secs_f64() * 1_000.0)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::output_path_for_request;

    #[test]
    fn uses_output_root_for_tile_exports() {
        assert_eq!(
            output_path_for_request(
                Path::new("14/8396_5421.glb"),
                None,
                Path::new("harbor/assets/tiles"),
            ),
            Path::new("harbor/assets/tiles/14/8396_5421.glb")
        );
    }

    #[test]
    fn output_glb_overrides_output_root() {
        assert_eq!(
            output_path_for_request(
                Path::new("14/8396_5421.glb"),
                Some(Path::new("single.glb")),
                Path::new("harbor/assets/tiles"),
            ),
            Path::new("single.glb")
        );
    }
}
