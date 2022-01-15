use glam::{vec3a, Vec2, Vec3A};
use opensubdiv_petite::far;
use rene_shader::Vertex;

use super::intermediate_scene::TriangleMesh;

fn generate_normal(mesh: &mut TriangleMesh) {
    for tri in mesh.indices.chunks(3) {
        let a = mesh.vertices[tri[0] as usize].position;
        let b = mesh.vertices[tri[1] as usize].position;
        let c = mesh.vertices[tri[2] as usize].position;

        let p = (b - a).cross(c - a);

        mesh.vertices[tri[0] as usize].normal += p;
        mesh.vertices[tri[1] as usize].normal += p;
        mesh.vertices[tri[2] as usize].normal += p;
    }

    for v in &mut mesh.vertices {
        v.normal = v.normal.normalize();
    }
}

pub fn loop_subdivision(mut mesh: TriangleMesh, level: usize) -> TriangleMesh {
    let num_vertices = mesh.vertices.len();
    let verts_per_face = vec![3; mesh.indices.len() / 3];

    let mut refiner = far::TopologyRefiner::new(
        // Populate the descriptor with our raw data.
        far::TopologyDescriptor::new(num_vertices as _, &verts_per_face, &mesh.indices),
        far::TopologyRefinerOptions {
            scheme: far::Scheme::Loop,
            ..Default::default()
        },
    )
    .expect("Could not create TopologyRefiner");

    refiner.refine_uniform(far::topology_refiner::UniformRefinementOptions {
        refinement_level: level,
        ..Default::default()
    });

    let primvar_refiner = far::PrimvarRefiner::new(&refiner);

    let mut refined_vertex = Vec::new();

    for v in mesh.vertices {
        refined_vertex.extend([v.position.x, v.position.y, v.position.z]);
    }

    for l in 1..=level {
        refined_vertex = primvar_refiner.interpolate(l, 3, &refined_vertex).unwrap();
    }

    let last_level = refiner.level(level).unwrap();

    mesh.vertices = refined_vertex
        .chunks(3)
        .map(|v| Vertex {
            position: vec3a(v[0], v[1], v[2]),
            normal: Vec3A::ZERO,
            uv: Vec2::ZERO,
        })
        .collect();

    mesh.indices = last_level
        .face_vertices_iter()
        .flat_map(|f| f.into_iter().copied())
        .collect();

    // TODO always generate normal?
    generate_normal(&mut mesh);

    mesh
}
