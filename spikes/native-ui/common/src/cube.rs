//! A grid of cubes as plain data: the mesh, where the cubes sit, and the camera.
//!
//! Two thousand cubes of 36 vertices each is about what a small schematic's mesh costs, drawn in
//! one instanced call, so the spike measures something closer to the real 3D preview than a
//! single spinning cube would.

/// The WGSL source of the shader both spikes compile.
pub const SHADER: &str = include_str!("cube.wgsl");

/// Cubes along each side of the grid.
pub const GRID_SIDE: usize = 16;

/// Bytes of one vertex: position (3 × f32) and normal (3 × f32).
pub const VERTEX_STRIDE: u64 = 24;

/// Bytes of one instance: position (3 × f32) and hue (f32).
pub const INSTANCE_STRIDE: u64 = 16;

/// Bytes of the uniform block: two 4×4 matrices.
pub const UNIFORM_BYTES: usize = 128;

pub type Matrix = [f32; 16];

/// The 36 vertices of a unit cube centred on the origin, counter-clockwise seen from outside.
#[must_use]
pub fn vertices() -> Vec<[f32; 6]> {
    // (normal, u, v) with u × v = normal
    const FACES: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]),
        ([-1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]),
        ([0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [1.0, 0.0, 0.0]),
        ([0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]),
        ([0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]),
        ([0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]),
    ];
    let mut out = Vec::with_capacity(36);
    for (normal, u, v) in FACES {
        let corner = |su: f32, sv: f32| -> [f32; 6] {
            let mut vertex = [0.0; 6];
            for axis in 0..3 {
                vertex[axis] = normal[axis] * 0.5 + u[axis] * 0.5 * su + v[axis] * 0.5 * sv;
                vertex[3 + axis] = normal[axis];
            }
            vertex
        };
        for (su, sv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            out.push(corner(su, sv));
        }
    }
    out
}

/// Where the cubes sit: x, y, z (centred on the origin) and a hue in 0..1. About half the cells
/// of the grid are filled, the same cells every time.
#[must_use]
pub fn instances(side: usize) -> Vec<[f32; 4]> {
    let middle = (side as f32 - 1.0) / 2.0;
    let mut out = Vec::new();
    for x in 0..side {
        for y in 0..side {
            for z in 0..side {
                let hash = (x as u32).wrapping_mul(73_856_093)
                    ^ (y as u32).wrapping_mul(19_349_663)
                    ^ (z as u32).wrapping_mul(83_492_791);
                if hash % 100 < 55 {
                    out.push([
                        x as f32 - middle,
                        y as f32 - middle,
                        z as f32 - middle,
                        ((hash >> 8) % 360) as f32 / 360.0,
                    ]);
                }
            }
        }
    }
    out
}

/// `a × b` for column-major matrices (`m[column * 4 + row]`).
#[must_use]
pub fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            out[column * 4 + row] = (0..4).map(|k| a[k * 4 + row] * b[column * 4 + k]).sum();
        }
    }
    out
}

/// A right-handed perspective projection with depth in 0..1 (what wgpu expects).
#[must_use]
pub fn perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> Matrix {
    let f = 1.0 / (fov_y / 2.0).tan();
    let mut m = [0.0; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = far / (near - far);
    m[11] = -1.0;
    m[14] = near * far / (near - far);
    m
}

#[must_use]
pub fn translation(x: f32, y: f32, z: f32) -> Matrix {
    let mut m = identity();
    m[12] = x;
    m[13] = y;
    m[14] = z;
    m
}

#[must_use]
pub fn rotation_y(angle: f32) -> Matrix {
    let (s, c) = angle.sin_cos();
    let mut m = identity();
    m[0] = c;
    m[2] = -s;
    m[8] = s;
    m[10] = c;
    m
}

#[must_use]
pub fn rotation_x(angle: f32) -> Matrix {
    let (s, c) = angle.sin_cos();
    let mut m = identity();
    m[5] = c;
    m[6] = s;
    m[9] = -s;
    m[10] = c;
    m
}

#[must_use]
pub fn identity() -> Matrix {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

/// The uniform block (model-view-projection, then the model matrix) for the grid turned by
/// `angle` radians, seen by a camera that frames the whole grid.
#[must_use]
pub fn uniforms(angle: f32, aspect: f32) -> [u8; UNIFORM_BYTES] {
    let model = multiply(&rotation_y(angle), &rotation_x(0.45));
    let distance = GRID_SIDE as f32 * 1.9;
    let view = translation(0.0, 0.0, -distance);
    let projection = perspective(45f32.to_radians(), aspect.max(0.01), 0.1, 300.0);
    let mvp = multiply(&projection, &multiply(&view, &model));
    let mut bytes = [0u8; UNIFORM_BYTES];
    for (slot, value) in mvp.iter().chain(model.iter()).enumerate() {
        bytes[slot * 4..slot * 4 + 4].copy_from_slice(&value.to_ne_bytes());
    }
    bytes
}

/// Any slice of `f32` arrays as bytes, for a GPU buffer.
#[must_use]
pub fn as_bytes<const N: usize>(data: &[[f32; N]]) -> Vec<u8> {
    data.iter()
        .flat_map(|row| row.iter().flat_map(|value| value.to_ne_bytes()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn apply(m: &Matrix, v: [f32; 4]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for (row, slot) in out.iter_mut().enumerate() {
            *slot = (0..4).map(|k| m[k * 4 + row] * v[k]).sum();
        }
        out
    }

    #[test]
    fn a_cube_has_thirty_six_vertices_on_unit_faces() {
        let mesh = vertices();
        assert_eq!(mesh.len(), 36);
        for v in &mesh {
            assert!(v[..3].iter().all(|c| c.abs() <= 0.5 + 1e-6));
            let normal_length = (v[3] * v[3] + v[4] * v[4] + v[5] * v[5]).sqrt();
            assert!((normal_length - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn every_triangle_faces_out() {
        let mesh = vertices();
        for triangle in mesh.chunks(3) {
            let [a, b, c] = [&triangle[0], &triangle[1], &triangle[2]];
            let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cross = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let dot = cross[0] * a[3] + cross[1] * a[4] + cross[2] * a[5];
            assert!(dot > 0.0, "{triangle:?}");
        }
    }

    #[test]
    fn about_half_of_the_grid_is_filled_the_same_way_every_time() {
        let cubes = instances(GRID_SIDE);
        assert_eq!(cubes, instances(GRID_SIDE));
        let cells = GRID_SIDE.pow(3);
        assert!(cubes.len() > cells * 45 / 100 && cubes.len() < cells * 65 / 100);
        assert!(cubes.iter().all(|c| (0.0..1.0).contains(&c[3])));
    }

    #[test]
    fn multiplying_by_the_identity_changes_nothing() {
        let m = multiply(&rotation_y(0.7), &translation(1.0, 2.0, 3.0));
        assert_eq!(multiply(&identity(), &m), m);
        assert_eq!(multiply(&m, &identity()), m);
    }

    #[test]
    fn a_quarter_turn_about_y_takes_x_to_minus_z() {
        let out = apply(&rotation_y(std::f32::consts::FRAC_PI_2), [1.0, 0.0, 0.0, 1.0]);
        assert!(out[0].abs() < 1e-6 && (out[2] + 1.0).abs() < 1e-6);
    }

    #[test]
    fn the_middle_of_the_grid_projects_to_the_middle_of_the_view_inside_the_depth_range() {
        let bytes = uniforms(1.0, 1.5);
        let mut mvp = [0.0f32; 16];
        for (slot, value) in mvp.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[slot * 4..slot * 4 + 4].try_into().unwrap());
        }
        let clip = apply(&mvp, [0.0, 0.0, 0.0, 1.0]);
        assert!((clip[0] / clip[3]).abs() < 1e-5 && (clip[1] / clip[3]).abs() < 1e-5);
        let depth = clip[2] / clip[3];
        assert!((0.0..1.0).contains(&depth), "{depth}");
    }

    #[test]
    fn the_whole_grid_is_in_front_of_the_camera_and_in_view() {
        let bytes = uniforms(0.0, 1.0);
        let mut mvp = [0.0f32; 16];
        for (slot, value) in mvp.iter_mut().enumerate() {
            *value = f32::from_ne_bytes(bytes[slot * 4..slot * 4 + 4].try_into().unwrap());
        }
        for cube in instances(GRID_SIDE) {
            let clip = apply(&mvp, [cube[0], cube[1], cube[2], 1.0]);
            assert!(clip[3] > 0.0);
            assert!((clip[0] / clip[3]).abs() <= 1.0 && (clip[1] / clip[3]).abs() <= 1.0);
        }
    }

    #[test]
    fn byte_helpers_have_the_sizes_the_pipelines_expect() {
        assert_eq!(as_bytes(&vertices()).len() as u64, 36 * VERTEX_STRIDE);
        assert_eq!(as_bytes(&instances(2)).len() as u64 % INSTANCE_STRIDE, 0);
        assert_eq!(uniforms(0.0, 1.0).len(), UNIFORM_BYTES);
    }
}
