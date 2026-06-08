use anyhow::{Context, Result, bail};
use earcutr::earcut;
use geo::{Coord, LineString, Polygon};

use crate::config::{BuildingConfig, LandConfig, TransportationConfig, WaterConfig};
use crate::tile::features::building::BuildingGeometry;
use crate::tile::features::land::LandGeometry;
use crate::tile::features::transportation::TransportationGeometry;
use crate::tile::features::water::WaterGeometry;

#[derive(Clone, Copy, Debug)]
pub(crate) struct SurfaceStyle {
    pub(crate) top: f32,
    pub(crate) bottom: Option<f32>,
}

#[derive(Debug, Default)]
pub(crate) struct MeshBuffers {
    pub(crate) positions: Vec<[f32; 3]>,
    pub(crate) normals: Vec<[f32; 3]>,
}

pub(crate) struct WaterMeshes {
    pub(crate) surface: MeshBuffers,
    pub(crate) bottom: MeshBuffers,
    pub(crate) side: MeshBuffers,
}

pub(crate) struct BuildingMeshes {
    pub(crate) roof: MeshBuffers,
    pub(crate) window: MeshBuffers,
}

#[derive(Clone, Copy)]
enum WallMode {
    All,
    SkipTileEdges,
}

impl MeshBuffers {
    pub(crate) fn push_triangle(
        &mut self,
        a: [f32; 3],
        mut b: [f32; 3],
        mut c: [f32; 3],
        normal: [f32; 3],
    ) {
        let actual = triangle_normal(a, b, c);
        if dot(actual, normal) < 0.0 {
            std::mem::swap(&mut b, &mut c);
        }

        self.positions.extend([a, b, c]);
        self.normals.extend([normal, normal, normal]);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.positions.is_empty()
    }
}

pub(crate) fn build_water_meshes(
    water: &WaterGeometry,
    config: &WaterConfig,
    shoreline_level: f32,
) -> Result<WaterMeshes> {
    let surface = build_polygon_mesh(
        &water.polygons.0,
        water.extent,
        config.xy_scale,
        SurfaceStyle {
            top: config.water_level,
            bottom: None,
        },
        "water surface",
    )?;
    let (bottom, side) = build_water_volume_meshes(
        &water.polygons.0,
        water.extent,
        config.xy_scale,
        shoreline_level,
        shoreline_level - config.depth,
    )?;

    Ok(WaterMeshes {
        surface,
        bottom,
        side,
    })
}

pub(crate) fn build_land_mesh(land: &LandGeometry, config: &LandConfig) -> Result<MeshBuffers> {
    build_polygon_mesh(
        &land.polygons.0,
        land.extent,
        config.xy_scale,
        SurfaceStyle {
            top: config.elevation,
            bottom: None,
        },
        "land",
    )
}

pub(crate) fn build_transportation_mesh(
    transportation: &TransportationGeometry,
    config: &TransportationConfig,
) -> Result<MeshBuffers> {
    build_polygon_mesh(
        &transportation.polygons.0,
        transportation.extent,
        config.xy_scale,
        SurfaceStyle {
            top: config.elevation,
            bottom: None,
        },
        "transportation",
    )
}

pub(crate) fn build_building_meshes(
    buildings: &BuildingGeometry,
    config: &BuildingConfig,
) -> Result<BuildingMeshes> {
    let mut roof = MeshBuffers::default();
    let mut window = MeshBuffers::default();

    for part in &buildings.parts {
        append_horizontal_surfaces(
            &mut roof,
            &part.polygon,
            buildings.extent,
            config.xy_scale,
            SurfaceStyle {
                top: part.top,
                bottom: None,
            },
        )?;
        append_side_walls(
            &mut window,
            &part.polygon,
            buildings.extent,
            config.xy_scale,
            part.top,
            part.bottom,
            WallMode::All,
        );
    }

    if roof.positions.is_empty() && window.positions.is_empty() {
        bail!("building mesh generation produced no triangles")
    }

    Ok(BuildingMeshes { roof, window })
}

fn build_polygon_mesh(
    polygons: &[Polygon<f64>],
    extent: u32,
    xy_scale: f32,
    style: SurfaceStyle,
    label: &str,
) -> Result<MeshBuffers> {
    let mut mesh = MeshBuffers::default();

    for polygon in polygons {
        append_horizontal_surfaces(&mut mesh, polygon, extent, xy_scale, style)?;
        if let Some(bottom) = style.bottom {
            append_side_walls(
                &mut mesh,
                polygon,
                extent,
                xy_scale,
                style.top,
                bottom,
                WallMode::All,
            );
        }
    }

    if mesh.positions.is_empty() {
        bail!("{label} mesh generation produced no triangles")
    }

    Ok(mesh)
}

fn build_water_volume_meshes(
    polygons: &[Polygon<f64>],
    extent: u32,
    xy_scale: f32,
    top: f32,
    bottom: f32,
) -> Result<(MeshBuffers, MeshBuffers)> {
    let mut bottom_mesh = MeshBuffers::default();
    let mut side_mesh = MeshBuffers::default();

    for polygon in polygons {
        append_bottom_surface(&mut bottom_mesh, polygon, extent, xy_scale, bottom)?;
        append_side_walls(
            &mut side_mesh,
            polygon,
            extent,
            xy_scale,
            top,
            bottom,
            WallMode::SkipTileEdges,
        );
    }

    if bottom_mesh.positions.is_empty() && side_mesh.positions.is_empty() {
        bail!("water volume mesh generation produced no triangles")
    }

    Ok((bottom_mesh, side_mesh))
}

fn append_horizontal_surfaces(
    mesh: &mut MeshBuffers,
    polygon: &Polygon<f64>,
    extent: u32,
    xy_scale: f32,
    style: SurfaceStyle,
) -> Result<()> {
    let mut coords = Vec::<f64>::new();
    let mut holes = Vec::<usize>::new();
    let mut vertices_2d = Vec::<[f32; 2]>::new();

    append_ring_for_triangulation(
        polygon.exterior(),
        extent,
        xy_scale,
        &mut coords,
        &mut vertices_2d,
        None,
    );

    for interior in polygon.interiors() {
        append_ring_for_triangulation(
            interior,
            extent,
            xy_scale,
            &mut coords,
            &mut vertices_2d,
            Some(&mut holes),
        );
    }

    let triangles = earcut(&coords, &holes, 2).context("failed to triangulate water polygon")?;
    for triangle in triangles.chunks_exact(3) {
        let a2 = vertices_2d[triangle[0]];
        let b2 = vertices_2d[triangle[1]];
        let c2 = vertices_2d[triangle[2]];

        let top_a = [a2[0], style.top, a2[1]];
        let top_b = [b2[0], style.top, b2[1]];
        let top_c = [c2[0], style.top, c2[1]];
        mesh.push_triangle(top_a, top_b, top_c, [0.0, 1.0, 0.0]);

        if let Some(bottom) = style.bottom {
            let bottom_a = [a2[0], bottom, a2[1]];
            let bottom_b = [b2[0], bottom, b2[1]];
            let bottom_c = [c2[0], bottom, c2[1]];
            mesh.push_triangle(bottom_a, bottom_c, bottom_b, [0.0, -1.0, 0.0]);
        }
    }

    Ok(())
}

fn append_bottom_surface(
    mesh: &mut MeshBuffers,
    polygon: &Polygon<f64>,
    extent: u32,
    xy_scale: f32,
    bottom: f32,
) -> Result<()> {
    let mut coords = Vec::<f64>::new();
    let mut holes = Vec::<usize>::new();
    let mut vertices_2d = Vec::<[f32; 2]>::new();

    append_ring_for_triangulation(
        polygon.exterior(),
        extent,
        xy_scale,
        &mut coords,
        &mut vertices_2d,
        None,
    );

    for interior in polygon.interiors() {
        append_ring_for_triangulation(
            interior,
            extent,
            xy_scale,
            &mut coords,
            &mut vertices_2d,
            Some(&mut holes),
        );
    }

    let triangles = earcut(&coords, &holes, 2).context("failed to triangulate water bottom")?;
    for triangle in triangles.chunks_exact(3) {
        let a2 = vertices_2d[triangle[0]];
        let b2 = vertices_2d[triangle[1]];
        let c2 = vertices_2d[triangle[2]];

        let bottom_a = [a2[0], bottom, a2[1]];
        let bottom_b = [b2[0], bottom, b2[1]];
        let bottom_c = [c2[0], bottom, c2[1]];
        mesh.push_triangle(bottom_a, bottom_c, bottom_b, [0.0, -1.0, 0.0]);
    }

    Ok(())
}

fn append_ring_for_triangulation(
    ring: &LineString<f64>,
    extent: u32,
    xy_scale: f32,
    coords: &mut Vec<f64>,
    vertices_2d: &mut Vec<[f32; 2]>,
    mut holes: Option<&mut Vec<usize>>,
) {
    let ring_points = ring_points(ring);
    if ring_points.is_empty() {
        return;
    }

    if let Some(holes) = holes.as_mut() {
        holes.push(vertices_2d.len());
    }

    for coord in ring_points {
        let point = tile_to_plane(*coord, extent, xy_scale);
        coords.push(point[0] as f64);
        coords.push(point[1] as f64);
        vertices_2d.push(point);
    }
}

fn append_side_walls(
    mesh: &mut MeshBuffers,
    polygon: &Polygon<f64>,
    extent: u32,
    xy_scale: f32,
    top: f32,
    bottom: f32,
    mode: WallMode,
) {
    append_ring_side_walls(
        mesh,
        polygon.exterior(),
        extent,
        xy_scale,
        top,
        bottom,
        mode,
    );
    for interior in polygon.interiors() {
        append_ring_side_walls(mesh, interior, extent, xy_scale, top, bottom, mode);
    }
}

fn append_ring_side_walls(
    mesh: &mut MeshBuffers,
    ring: &LineString<f64>,
    extent: u32,
    xy_scale: f32,
    top: f32,
    bottom: f32,
    mode: WallMode,
) {
    let points = ring_points(ring);
    if points.len() < 3 {
        return;
    }

    let world_points: Vec<_> = points
        .iter()
        .map(|coord| tile_to_plane(*coord, extent, xy_scale))
        .collect();
    let sign = signed_area_2d(&world_points).signum();

    for index in 0..world_points.len() {
        if matches!(mode, WallMode::SkipTileEdges)
            && edge_on_tile_boundary(points[index], points[(index + 1) % points.len()], extent)
        {
            continue;
        }

        let start = world_points[index];
        let end = world_points[(index + 1) % world_points.len()];
        let edge = [end[0] - start[0], end[1] - start[1]];
        let normal = if sign >= 0.0 {
            normalize3([edge[1], 0.0, -edge[0]])
        } else {
            normalize3([-edge[1], 0.0, edge[0]])
        };

        let top_start = [start[0], top, start[1]];
        let top_end = [end[0], top, end[1]];
        let bottom_start = [start[0], bottom, start[1]];
        let bottom_end = [end[0], bottom, end[1]];

        mesh.push_triangle(top_start, bottom_start, bottom_end, normal);
        mesh.push_triangle(top_start, bottom_end, top_end, normal);
    }
}

fn ring_points(ring: &LineString<f64>) -> &[Coord<f64>] {
    let coords = &ring.0;
    if coords.len() >= 2 && coords.first() == coords.last() {
        &coords[..coords.len() - 1]
    } else {
        coords.as_slice()
    }
}

fn tile_to_plane(coord: Coord<f64>, extent: u32, xy_scale: f32) -> [f32; 2] {
    [
        (extent as f32 - coord.x as f32) * xy_scale,
        (extent as f32 - coord.y as f32) * xy_scale,
    ]
}

fn edge_on_tile_boundary(start: Coord<f64>, end: Coord<f64>, extent: u32) -> bool {
    let extent = extent as f64;

    ((start.x - 0.0).abs() <= f64::EPSILON && (end.x - 0.0).abs() <= f64::EPSILON)
        || ((start.x - extent).abs() <= f64::EPSILON && (end.x - extent).abs() <= f64::EPSILON)
        || ((start.y - 0.0).abs() <= f64::EPSILON && (end.y - 0.0).abs() <= f64::EPSILON)
        || ((start.y - extent).abs() <= f64::EPSILON && (end.y - extent).abs() <= f64::EPSILON)
}

fn signed_area_2d(points: &[[f32; 2]]) -> f32 {
    if points.len() < 3 {
        return 0.0;
    }

    let mut area = 0.0;
    for index in 0..points.len() {
        let current = points[index];
        let next = points[(index + 1) % points.len()];
        area += current[0] * next[1] - next[0] * current[1];
    }
    area * 0.5
}

fn triangle_normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    normalize3([
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ])
}

fn normalize3(vector: [f32; 3]) -> [f32; 3] {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if length <= f32::EPSILON {
        [0.0, 0.0, 0.0]
    } else {
        [vector[0] / length, vector[1] / length, vector[2] / length]
    }
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[cfg(test)]
mod tests {
    use geo::{Coord, LineString, MultiPolygon, Polygon};

    use crate::tile::features::building::BuildingPart;

use super::*;

    #[test]
    fn builds_closed_water_mesh_for_rectangle() {
        let water = WaterGeometry {
            extent: 10,
            polygons: MultiPolygon(vec![rectangle(1.0, 1.0, 4.0, 3.0)]),
        };
        let meshes =
            build_water_meshes(&water, &WaterConfig::default(), 0.0).expect("mesh should build");

        assert!(!meshes.surface.positions.is_empty());
        assert!(!meshes.bottom.positions.is_empty());
        assert_eq!(meshes.surface.positions.len(), meshes.surface.normals.len());
        assert_eq!(meshes.bottom.positions.len(), meshes.bottom.normals.len());
        assert!(
            meshes
                .surface
                .positions
                .iter()
                .all(|vertex| vertex[1] == -0.1)
        );
        assert!(meshes.side.positions.iter().any(|vertex| vertex[1] == 0.0));
        assert!(
            meshes
                .bottom
                .positions
                .iter()
                .any(|vertex| vertex[1] == -5.0)
        );
    }

    #[test]
    fn builds_land_surface_without_bottom_faces() {
        let land = LandGeometry {
            extent: 10,
            polygons: MultiPolygon(vec![rectangle(1.0, 1.0, 4.0, 3.0)]),
        };
        let mesh = build_land_mesh(&land, &LandConfig::default()).expect("mesh should build");

        assert!(!mesh.positions.is_empty());
        assert!(mesh.positions.iter().all(|vertex| vertex[1] == 0.0));
    }

    #[test]
    fn skips_water_walls_on_tile_edges() {
        let water = WaterGeometry {
            extent: 10,
            polygons: MultiPolygon(vec![rectangle(0.0, 0.0, 10.0, 3.0)]),
        };
        let meshes =
            build_water_meshes(&water, &WaterConfig::default(), 0.0).expect("mesh should build");

        assert_eq!(meshes.side.positions.len(), 6);
        assert!(
            meshes
                .bottom
                .positions
                .iter()
                .any(|vertex| vertex[1] == -5.0)
        );
    }

    #[test]
    fn building_walls_include_closing_edge() {
        let buildings = BuildingGeometry {
            extent: 10,
            parts: vec![BuildingPart {
                polygon: rectangle(1.0, 1.0, 4.0, 3.0),
                bottom: 0.0,
                top: 5.0,
            }],
        };
        let meshes = build_building_meshes(&buildings, &BuildingConfig::default())
            .expect("mesh should build");

        let left_wall_vertices = meshes
            .window
            .positions
            .iter()
            .filter(|vertex| (vertex[0] - 9.0).abs() < f32::EPSILON)
            .count();

        assert!(left_wall_vertices >= 6);
        assert!(!meshes.roof.positions.is_empty());
    }

    fn rectangle(min_x: f64, min_y: f64, max_x: f64, max_y: f64) -> Polygon<f64> {
        Polygon::new(
            LineString::new(vec![
                Coord { x: min_x, y: min_y },
                Coord { x: max_x, y: min_y },
                Coord { x: max_x, y: max_y },
                Coord { x: min_x, y: max_y },
                Coord { x: min_x, y: min_y },
            ]),
            vec![],
        )
    }
}
