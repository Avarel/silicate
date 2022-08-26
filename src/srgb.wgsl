// Vertex shader ///////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
// Nothing special about this section. It gets fed vertices in a triangle strip
// configuration to draw a square on the texture.

type vec2f = vec2<f32>;
type vec3f = vec3<f32>;
type vec4f = vec4<f32>;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) bg_coords: vec2f,
    @location(2) fg_coords: vec2f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) bg_coords: vec2f,
    @location(1) fg_coords: vec2f,
};

@vertex
fn vs_main(
    model: VertexInput
) -> VertexOutput {
    var out: VertexOutput;
    out.bg_coords = model.bg_coords;
    out.fg_coords = model.fg_coords;
    out.clip_position = vec4(model.position, 1.0);
    return out;
}

@group(0) @binding(0)
var splr: sampler;
@group(1) @binding(0)
var composite: texture_2d<f32>;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    let bga = textureSample(composite, splr, in.bg_coords);
    return pow(bga, vec4(vec3(2.2), 1.0));
}
