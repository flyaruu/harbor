use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use gltf_json as gltf;
use gltf_json::validation::{Checked, USize64};

use crate::mesh::MeshBuffers;

pub(crate) struct SceneMesh<'a> {
    pub(crate) mesh: &'a MeshBuffers,
    pub(crate) material_tag: &'a str,
    pub(crate) base_color: [f32; 4],
}

pub(crate) fn write_glb(path: &Path, meshes: &[SceneMesh<'_>]) -> Result<()> {
    if meshes.is_empty() {
        anyhow::bail!("no meshes were provided for GLB export");
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create output directory {}", parent.display()))?;
    }

    let mut binary = Vec::new();
    let mut specs = Vec::with_capacity(meshes.len());

    for scene_mesh in meshes {
        let position_offset = binary.len();
        for position in &scene_mesh.mesh.positions {
            for component in position {
                binary.extend_from_slice(&component.to_le_bytes());
            }
        }
        pad_bytes(&mut binary, 4, 0);

        let normal_offset = binary.len();
        for normal in &scene_mesh.mesh.normals {
            for component in normal {
                binary.extend_from_slice(&component.to_le_bytes());
            }
        }
        pad_bytes(&mut binary, 4, 0);

        specs.push(MeshSpec {
            position_offset,
            normal_offset,
            position_count: scene_mesh.mesh.positions.len(),
            normal_count: scene_mesh.mesh.normals.len(),
            bounds: mesh_bounds(&scene_mesh.mesh.positions).context("mesh had no vertices")?,
            material_tag: scene_mesh.material_tag.to_string(),
            base_color: scene_mesh.base_color,
        });
    }

    let root = build_gltf_root(binary.len(), &specs)?;

    let mut json_bytes = root.to_vec().context("failed to serialize glTF JSON")?;
    pad_bytes(&mut json_bytes, 4, b' ');

    let total_length = 12 + 8 + json_bytes.len() + 8 + binary.len();
    let mut glb = Vec::with_capacity(total_length);
    glb.extend_from_slice(b"glTF");
    glb.extend_from_slice(&2u32.to_le_bytes());
    glb.extend_from_slice(&(total_length as u32).to_le_bytes());

    glb.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"JSON");
    glb.extend_from_slice(&json_bytes);

    glb.extend_from_slice(&(binary.len() as u32).to_le_bytes());
    glb.extend_from_slice(b"BIN\0");
    glb.extend_from_slice(&binary);

    fs::write(path, glb).with_context(|| format!("failed to write {}", path.display()))
}

struct MeshSpec {
    position_offset: usize,
    normal_offset: usize,
    position_count: usize,
    normal_count: usize,
    bounds: ([f32; 3], [f32; 3]),
    material_tag: String,
    base_color: [f32; 4],
}

fn build_gltf_root(binary_len: usize, meshes: &[MeshSpec]) -> Result<gltf::Root> {
    let mut root = gltf::Root {
        asset: gltf::Asset {
            generator: Some("osm_pbf_processor".to_string()),
            ..Default::default()
        },
        ..Default::default()
    };
    let buffer_index = root.push(gltf::Buffer {
        byte_length: USize64::from(binary_len),
        name: None,
        uri: None,
        extensions: None,
        extras: Default::default(),
    });

    let mut scene_nodes = Vec::with_capacity(meshes.len());
    for spec in meshes {
        let position_view = root.push(gltf::buffer::View {
            buffer: buffer_index,
            byte_length: USize64::from(spec.normal_offset - spec.position_offset),
            byte_offset: Some(USize64::from(spec.position_offset)),
            byte_stride: None,
            target: Some(Checked::Valid(gltf::buffer::Target::ArrayBuffer)),
            extensions: None,
            extras: Default::default(),
            name: None,
        });

        let normal_view = root.push(gltf::buffer::View {
            buffer: buffer_index,
            byte_length: USize64::from(spec.normal_count * std::mem::size_of::<[f32; 3]>()),
            byte_offset: Some(USize64::from(spec.normal_offset)),
            byte_stride: None,
            target: Some(Checked::Valid(gltf::buffer::Target::ArrayBuffer)),
            extensions: None,
            extras: Default::default(),
            name: None,
        });

        let position_accessor = root.push(gltf::Accessor {
            buffer_view: Some(position_view),
            byte_offset: Some(USize64::from(0usize)),
            count: USize64::from(spec.position_count),
            component_type: Checked::Valid(gltf::accessor::GenericComponentType(
                gltf::accessor::ComponentType::F32,
            )),
            extensions: None,
            extras: Default::default(),
            type_: Checked::Valid(gltf::accessor::Type::Vec3),
            min: Some(gltf::serialize::to_value(spec.bounds.0)?),
            max: Some(gltf::serialize::to_value(spec.bounds.1)?),
            name: None,
            normalized: false,
            sparse: None,
        });

        let normal_accessor = root.push(gltf::Accessor {
            buffer_view: Some(normal_view),
            byte_offset: Some(USize64::from(0usize)),
            count: USize64::from(spec.normal_count),
            component_type: Checked::Valid(gltf::accessor::GenericComponentType(
                gltf::accessor::ComponentType::F32,
            )),
            extensions: None,
            extras: Default::default(),
            type_: Checked::Valid(gltf::accessor::Type::Vec3),
            min: None,
            max: None,
            name: None,
            normalized: false,
            sparse: None,
        });

        let material_index = root.push(gltf::Material {
            alpha_mode: Checked::Valid(if spec.base_color[3] < 1.0 {
                gltf::material::AlphaMode::Blend
            } else {
                gltf::material::AlphaMode::Opaque
            }),
            double_sided: true,
            name: Some(spec.material_tag.clone()),
            pbr_metallic_roughness: gltf::material::PbrMetallicRoughness {
                base_color_factor: gltf::material::PbrBaseColorFactor(spec.base_color),
                metallic_factor: gltf::material::StrengthFactor(0.0),
                roughness_factor: gltf::material::StrengthFactor(0.2),
                ..Default::default()
            },
            ..Default::default()
        });

        let mut attributes = BTreeMap::new();
        attributes.insert(
            Checked::Valid(gltf::mesh::Semantic::Positions),
            position_accessor,
        );
        attributes.insert(
            Checked::Valid(gltf::mesh::Semantic::Normals),
            normal_accessor,
        );

        let mesh_index = root.push(gltf::Mesh {
            extensions: None,
            extras: Default::default(),
            name: Some(spec.material_tag.clone()),
            primitives: vec![gltf::mesh::Primitive {
                attributes,
                extensions: None,
                extras: Default::default(),
                indices: None,
                material: Some(material_index),
                mode: Checked::Valid(gltf::mesh::Mode::Triangles),
                targets: None,
            }],
            weights: None,
        });

        let node_index = root.push(gltf::Node {
            mesh: Some(mesh_index),
            name: Some(spec.material_tag.clone()),
            ..Default::default()
        });
        scene_nodes.push(node_index);
    }

    let scene_index = root.push(gltf::Scene {
        extensions: None,
        extras: Default::default(),
        name: None,
        nodes: scene_nodes,
    });
    root.scene = Some(scene_index);

    Ok(root)
}

fn mesh_bounds(positions: &[[f32; 3]]) -> Option<([f32; 3], [f32; 3])> {
    let first = *positions.first()?;
    let mut min = first;
    let mut max = first;

    for position in positions.iter().copied().skip(1) {
        min[0] = min[0].min(position[0]);
        min[1] = min[1].min(position[1]);
        min[2] = min[2].min(position[2]);
        max[0] = max[0].max(position[0]);
        max[1] = max[1].max(position[1]);
        max[2] = max[2].max(position[2]);
    }

    Some((min, max))
}

fn pad_bytes(bytes: &mut Vec<u8>, alignment: usize, value: u8) {
    let remainder = bytes.len() % alignment;
    if remainder != 0 {
        bytes.resize(bytes.len() + alignment - remainder, value);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::*;

    #[test]
    fn writes_valid_glb_header() {
        let mut mesh = MeshBuffers::default();
        mesh.push_triangle(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
        );

        let output =
            Path::new("/var/folders/s5/f3fvtwcx4j7f3clmdcn12cww0000gn/T/opencode/test-water.glb");
        write_glb(
            output,
            &[SceneMesh {
                mesh: &mesh,
                material_tag: "test_water",
                base_color: [0.1, 0.35, 0.8, 1.0],
            }],
        )
        .expect("glb should write");

        let bytes = fs::read(output).expect("glb should exist");
        assert_eq!(&bytes[..4], b"glTF");
    }

    #[test]
    fn creates_parent_directories_before_writing() {
        let mut mesh = MeshBuffers::default();
        mesh.push_triangle(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 0.0],
        );

        let output = Path::new(
            "/var/folders/s5/f3fvtwcx4j7f3clmdcn12cww0000gn/T/opencode/output/14/8396_5421.glb",
        );
        if let Some(parent) = output.parent() {
            let _ = fs::remove_dir_all(parent);
        }

        write_glb(
            output,
            &[SceneMesh {
                mesh: &mesh,
                material_tag: "test_water",
                base_color: [0.1, 0.35, 0.8, 1.0],
            }],
        )
        .expect("glb should write into nested directory");

        assert!(output.exists());
    }
}
