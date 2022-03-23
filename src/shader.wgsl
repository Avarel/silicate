// Vertex shader

struct VertexInput {
    [[location(0)]] position: vec3<f32>;
    [[location(1)]] tex_coords: vec2<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] tex_coords: vec2<f32>;
};

struct InstanceInput {
    [[location(5)]] pos: vec2<f32>;
    [[location(6)]] layer: u32;
};

[[stage(vertex)]]
fn vs_main(
    model: VertexInput, instance: InstanceInput
) -> VertexOutput {
    let delta: vec3<f32> = vec3<f32>(instance.pos[0], instance.pos[1], 0.0);
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.clip_position = vec4<f32>(model.position + delta, 1.0);
    return out;
}
 
// Fragment shader

[[group(0), binding(0)]]
var t_diffuse: texture_2d_array<f32>;
[[group(0), binding(1)]]
var s_diffuse: sampler;

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    return textureSample(t_diffuse, s_diffuse, in.tex_coords, 0);
}