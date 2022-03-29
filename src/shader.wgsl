// Vertex shader

struct VertexInput {
    [[location(0)]] position: vec3<f32>;
    [[location(1)]] tex_coords: vec2<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] tex_coords: vec2<f32>;
};

[[stage(vertex)]]
fn vs_main(
    model: VertexInput
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.clip_position = vec4<f32>(model.position, 1.0);
    return out;
}
 
// Fragment shader

// All components are in the range [0…1], including hue.
fn rgb2hsv(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.bg, K.wz), vec4<f32>(c.gb, K.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), step(p.x, c.r));

    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

// All components are in the range [0…1], including hue.
fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, vec3<f32>(0.0), vec3<f32>(1.0)), c.y);
}

fn comp(c: vec3<f32>, a: f32) -> vec3<f32> {
    return c * (1.0 - a);
}

fn normal(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return s;
}

fn multiply(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return s * b;
}

fn screen(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return s + b - s * b;
}

fn hard_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return mix(
        multiply(b, s * 2.0),
        screen(b, 2.0 * s - vec3<f32>(1.0)),
        step(b, vec3<f32>(0.5))
    );
}

fn overlay(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return hard_light(s, b);
}

fn darken(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return min(s, b);
}

fn lighten(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return max(s, b);
}

fn difference(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return abs(b - s);
}

fn exclusion(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return b + s - 2.0 * b * s;
}

fn color_dodge(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    return select(
        min(vec3<f32>(1.0), b / (1.0 - s)),
        vec3<f32>(1.0),
        vec3<bool>(s.r >= 1.0, s.g >= 1.0, s.b >= 1.0)
    );
}

fn soft_light(b: vec3<f32>, s: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return mix(
        sqrt(b) * (2.0 * s - 1.0) + 2.0 * b * (1.0 - s), 
        2.0 * b * s + b * b * (1.0 - 2.0 * s) + comp(s, da) + comp(b, sa),
        step(b, vec3<f32>(0.5))
    );
}

struct CtxInput {
    opacity: f32;
    blend: u32;
};

[[group(0), binding(0)]]
var splr: sampler;
[[group(1), binding(0)]]
var composite: texture_2d<f32>;
[[group(1), binding(1)]]
var clipping_mask: texture_2d<f32>;
[[group(1), binding(2)]]
var layer: texture_2d<f32>;
[[group(1), binding(3)]]
var<uniform> ctx: CtxInput;

[[stage(fragment)]]
fn fs_main(in: VertexOutput) -> [[location(0)]] vec4<f32> {
    var fg = textureSample(layer, splr, in.tex_coords);
    let maska = textureSample(clipping_mask, splr, in.tex_coords).a;
    fg.a = min(fg.a, maska);

    let bg = textureSample(composite, splr, in.tex_coords);

    // Procreate uses premultiplied alpha, so unpremultiply it.
    let bg_raw = clamp(bg.rgb / bg.a, vec3<f32>(0.0), vec3<f32>(1.0));
    let fg_raw = clamp(fg.rgb / fg.a, vec3<f32>(0.0), vec3<f32>(1.0));

    var final_pixel = vec3<f32>(0.0);

    switch (ctx.blend) {
        case 1: {
            final_pixel = multiply(bg_raw, fg_raw);
        }
        case 2: {
            final_pixel = screen(bg_raw, fg_raw);
        }
        case 4: {
            final_pixel = lighten(bg_raw, fg_raw);
        }
        case 5: {
            final_pixel = exclusion(bg_raw, fg_raw);
        }
        case 6: {
            final_pixel = difference(bg_raw, fg_raw);
        }
        case 9: {
            final_pixel = color_dodge(bg_raw, fg_raw);
        }
        case 11: {
            final_pixel = overlay(bg_raw, fg_raw);
        }
        case 12: {
            final_pixel = hard_light(bg_raw, fg_raw);
        }
        case 17: {
            final_pixel = soft_light(bg_raw, fg_raw, 1.0, 1.0);
        }
        case 19: {
            final_pixel = darken(bg_raw, fg_raw);
        }
        default: {
            final_pixel = normal(bg_raw, fg_raw);
        }
    }
    let a_fg = fg.a * ctx.opacity;
    return vec4<f32>(final_pixel * a_fg + bg.rgb * (1.0 - a_fg), bg.a + a_fg - bg.a * a_fg);
}
