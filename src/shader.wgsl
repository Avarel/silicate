// Vertex shader

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
 
// Fragment shader

// All components are in the range [0…1], including hue.
fn rgb2hsv(c: vec3f) -> vec3f {
    let K = vec4(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4(c.bg, K.wz), vec4(c.gb, K.xy), step(c.b, c.g));
    let q = mix(vec4(p.xyw, c.r), vec4(c.r, p.yzx), step(p.x, c.r));

    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec3(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

// All components are in the range [0…1], including hue.
fn hsv2rgb(c: vec3f) -> vec3f {
    let K = vec4(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, vec3(0.0), vec3(1.0)), c.y);
}

fn comp(c: vec3f, a: f32) -> vec3f {
    return c * (1.0 - a);
}

fn stdalpha(b: f32, f: f32) -> f32 {
    return b + f - b * f;
}

fn normal(b: vec3f, s: vec3f) -> vec3f {
    return s;
}

fn multiply(b: vec3f, s: vec3f) -> vec3f {
    return s * b;
}

fn screen(b: vec3f, s: vec3f) -> vec3f {
    return s + b - s * b;
}

fn add(b: vec3f, s: vec3f) -> vec3f {
    return s + b;
}

fn hard_light(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        multiply(b, s * 2.0),
        screen(b, 2.0 * s - vec3(1.0)),
        step(b, vec3(0.5))
    );
}

fn overlay(b: vec3f, s: vec3f) -> vec3f {
    return hard_light(s, b);
}

fn darken(b: vec3f, s: vec3f) -> vec3f {
    return min(s, b);
}

fn lighten(b: vec3f, s: vec3f) -> vec3f {
    return max(s, b);
}

fn difference(b: vec3f, s: vec3f) -> vec3f {
    return abs(b - s);
}

fn subtract(b: vec3f, s: vec3f) -> vec3f {
    return b - s;
}

fn linear_burn(b: vec3f, s: vec3f) -> vec3f {
    return max(b + s - vec3(1.0), vec3(0.0));
}

fn exclusion(b: vec3f, s: vec3f) -> vec3f {
    return b + s - 2.0 * b * s;
}

fn color_dodge(b: vec3f, s: vec3f) -> vec3f {
    return select(
        min(vec3(1.0), b / (1.0 - s)),
        vec3(1.0),
        vec3(s.r >= 1.0, s.g >= 1.0, s.b >= 1.0)
    );
}

fn color_burn(b: vec3f, s: vec3f) -> vec3f {
    return select(
        max(vec3(0.0), (vec3(1.0) - ((vec3(1.0) - b)/s))),
        vec3(1.0),
        vec3(s.r <= 0.0, s.g <= 0.0, s.b <= 0.0)
    );
}

fn soft_light(b: vec3f, s: vec3f, da: f32, sa: f32) -> vec3f {
    return mix(
        sqrt(b) * (2.0 * s - 1.0) + 2.0 * b * (1.0 - s), 
        2.0 * b * s + b * b * (1.0 - 2.0 * s) + comp(s, da) + comp(b, sa),
        step(b, vec3(0.5))
    );
}

struct CtxInput {
    opacity: f32,
    blend: u32,
};

@group(0) @binding(0)
var splr: sampler;
@group(1) @binding(0)
var composite: texture_2d<f32>;
@group(1) @binding(1)
var clipping_mask: texture_2d<f32>;
@group(1) @binding(2)
var layer: texture_2d<f32>;
@group(1) @binding(3)
var<uniform> ctx: CtxInput;

// Blend alpha straight colors
fn premultiplied_blend(bg: vec4f, fg: vec4f, cg: vec4f) -> vec4f {
    return clamp(vec4(
        cg.rgb * cg.a * bg.a + comp(fg.rgb, bg.a) + comp(bg.rgb, cg.a),
        stdalpha(bg.a, cg.a)
    ), vec4(0.0), vec4(1.0));
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Premultiplied colors
    let bga = textureSample(composite, splr, in.bg_coords);
    var fga = textureSample(layer, splr, in.fg_coords);
    let maska = textureSample(clipping_mask, splr, in.fg_coords).a;

    if (bga.a == 0.0) {
        return fga;
    } else if (fga.a == 0.0) {
        return bga;
    }

    // Procreate uses premultiplied alpha, so unpremultiply it.
    let bg = vec4(clamp(bga.rgb / bga.a, vec3(0.0), vec3(1.0)), bga.a);
    var fg = vec4(clamp(fga.rgb / fga.a, vec3(0.0), vec3(1.0)), min(fga.a, maska) * ctx.opacity);

    // Some blending functions work with premultiplied alpha (to avoid data loss due to division)
    // So fix the premultiplied foreground's color.
    fga.a = min(fga.a, maska) * ctx.opacity;

    // Blend straight colors according to modes
    var final_pixel = vec3(0.0);
    switch (ctx.blend) {
        case 1u: { final_pixel = multiply(bg.rgb, fg.rgb); }
        case 2u: { final_pixel = screen(bg.rgb, fg.rgb); }
        case 3u: { final_pixel = add(bg.rgb, fg.rgb); }
        case 4u: { final_pixel = lighten(bg.rgb, fg.rgb); }
        case 5u: { final_pixel = exclusion(bg.rgb, fg.rgb); }
        case 6u: { final_pixel = difference(bg.rgb, fg.rgb); }
        case 7u: { final_pixel = subtract(bg.rgb, fg.rgb); }
        case 8u: { final_pixel = linear_burn(bg.rgb, fg.rgb); }
        case 9u: { final_pixel = color_dodge(bg.rgb, fg.rgb); }
        case 10u: { final_pixel = color_burn(bg.rgb, fg.rgb); }
        case 11u: { final_pixel = overlay(bg.rgb, fg.rgb); }
        case 12u: { final_pixel = hard_light(bg.rgb, fg.rgb); }
        case 17u: { final_pixel = soft_light(bg.rgb, fg.rgb, 1.0, 1.0); }
        case 19u: { final_pixel = darken(bg.rgb, fg.rgb); }
        default: { final_pixel = normal(bg.rgb, fg.rgb); }
    }
    final_pixel = clamp(final_pixel, vec3(0.0), vec3(1.0));

    // Compute final premultiplied colors
    return premultiplied_blend(bga, fga, vec4(final_pixel, fg.a));
}
