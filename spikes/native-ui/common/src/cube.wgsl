// A grid of lit cubes, drawn with one instanced call: a stand-in for a schematic's voxel mesh.

struct Uniforms {
    mvp: mat4x4<f32>,
    model: mat4x4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VertexIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    // xyz: where this cube sits, w: its hue
    @location(2) instance: vec4<f32>,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) colour: vec3<f32>,
};

fn hue(h: f32) -> vec3<f32> {
    let k = fract(vec3<f32>(h) + vec3<f32>(0.0, 2.0 / 3.0, 1.0 / 3.0)) * 6.0;
    return clamp(abs(k - vec3<f32>(3.0)) - vec3<f32>(1.0), vec3<f32>(0.0), vec3<f32>(1.0));
}

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    out.clip = u.mvp * vec4<f32>(in.position * 0.9 + in.instance.xyz, 1.0);
    let normal = normalize((u.model * vec4<f32>(in.normal, 0.0)).xyz);
    let light = normalize(vec3<f32>(0.4, 0.8, 0.6));
    let shade = 0.35 + 0.65 * max(dot(normal, light), 0.0);
    out.colour = mix(vec3<f32>(0.55), hue(in.instance.w), 0.75) * shade;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(in.colour, 1.0);
}
