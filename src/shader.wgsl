// Vertex shader ///////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
// Nothing special about this section. It gets fed vertices in a triangle strip
// configuration to draw a square on the texture.

alias vec2f = vec2<f32>;
alias vec3f = vec3<f32>;
alias vec4f = vec4<f32>;

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
 
// Blending code ///////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////

// HSL Blending Modes //////////////////////////////////////////////////////////
// [PDF Blend Modes: Addendum]
// [KHR_blend_equation_advanced]
fn lum(c: vec3f) -> f32 {
    return dot(c, vec3(0.3, 0.59, 0.11));
}

fn clip_color(c: vec3f) -> vec3f {
    let l = lum(c);
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    var z = c;
    if (n < 0.0) {
        z = l + (((c - l) * l) / (l - n));
    }
    if (x > 1.0) {
        z = l + (((z - l) * (1.0 - l)) / (x - l));
    }
    return clamp(z, vec3(0.0), vec3(1.0));
}

fn set_lum(c: vec3f, l: f32) -> vec3f {
    let d = l - lum(c);
    return clip_color(c + d);
}

fn sat(c: vec3f) -> f32 {
    let n = min(min(c.r, c.g), c.b);
    let x = max(max(c.r, c.g), c.b);
    return x - n;
}

fn set_sat(cb: vec3f, s: f32) -> vec3f {
    let mb = min(min(cb.r, cb.g), cb.b);
    let sb = sat(cb);
    // Equivalent (modulo rounding errors) to setting the
    // smallest (R,G,B) component to 0, the largest to <ssat>,
    // and interpolating the "middle" component based on its
    // original value relative to the smallest/largest.
    return select(vec3(0.0), (cb - mb) * s / sb, sb > 0.0);
}

fn color(b: vec3f, s: vec3f) -> vec3f {
    return set_lum(s.rgb, lum(b.rgb));
}

fn luminosity(b: vec3f, s: vec3f) -> vec3f {
    return set_lum(b.rgb, lum(s.rgb));
}

fn hue(b: vec3f, s: vec3f) -> vec3f {
    return set_lum(set_sat(s.rgb, sat(b.rgb)), lum(b.rgb));
}

fn saturation(b: vec3f, s: vec3f) -> vec3f {
    return set_lum(set_sat(b.rgb, sat(s.rgb)), lum(b.rgb));
}

// Utilities ///////////////////////////////////////////////////////////////////
fn comp(c: vec3f, a: f32) -> vec3f {
    return c * (1.0 - a);
}

fn stdalpha(b: f32, f: f32) -> f32 {
    return b + f - b * f;
}

// RGB Blending Modes //////////////////////////////////////////////////////////
// [PDF Blend Modes: Addendum]
// [https://photoblogstop.com/photoshop/photoshop-blend-modes-explained]

fn normal(b: vec3f, s: vec3f) -> vec3f {
    return s;
}

fn multiply(b: vec3f, s: vec3f) -> vec3f {
    return s * b;
}

fn divide(b: vec3f, s: vec3f) -> vec3f {
    return b / s;
}

fn screen(b: vec3f, s: vec3f) -> vec3f {
    return s + b - s * b;
}

fn add(b: vec3f, s: vec3f) -> vec3f {
    return s + b;
}

fn hard_light(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        screen(b, 2.0 * s - 1.0),
        multiply(b, s * 2.0),
        step(s, vec3(0.5))
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
    return max(b + s - 1.0, vec3(0.0));
}

fn linear_dodge(b: vec3f, s: vec3f) -> vec3f {
    return min(b + s, vec3(1.0));
}

fn linear_light(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        linear_dodge(b, 2.0 * (s - 0.5)), 
        linear_burn(b, 2.0 * s),
        step(s, vec3(0.5))
    );
}

fn exclusion(b: vec3f, s: vec3f) -> vec3f {
    return b + s - 2.0 * b * s;
}

fn color_dodge(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        vec3(1.0),
        min(vec3(1.0), b / (1.0 - s)),
        step(s, vec3(1.0))
    );
}

fn color_burn(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        1.0 - min(vec3(1.0), (1.0 - b) / s),
        vec3(0.0),
        step(s, vec3(0.0))
    );
}

fn soft_light(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        sqrt(b) * (2.0 * s - 1.0) + 2.0 * b * (1.0 - s), 
        2.0 * b * s + b * b * (1.0 - 2.0 * s),
        step(s, vec3(0.5))
    );
}

fn vivid_light(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        color_dodge(b, 2.0 * (s - 0.5)),
        color_burn(b, 2.0 * s),
        step(s, vec3(0.5))
    );
}

fn hard_mix(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        vec3(1.0), 
        vec3(0.0), 
        step(vivid_light(b, s), vec3(0.5))
    );
}

fn pin_light(b: vec3f, s: vec3f) -> vec3f {
    return mix(
        lighten(b, 2.0 * (s - 0.5)),
        darken(b, 2.0 * s),
        step(s, vec3(0.5))
    );
}

fn lighter_color(b: vec3f, s: vec3f) -> vec3f {
    return select(b, s, lum(b) < lum(s));
}

fn darker_color(b: vec3f, s: vec3f) -> vec3f {
    return select(b, s, lum(b) > lum(s));
}

// Fragment shader /////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////

@group(0) @binding(0)
var splr: sampler;
@group(1) @binding(0)
var composite: texture_2d<f32>;
@group(1) @binding(1)
var textures: texture_2d_array<f32>;
@group(1) @binding(2)
var<storage, read> layers: array<u32>;
@group(1) @binding(3)
var<storage, read> masks: array<u32>;
@group(1) @binding(4)
var<storage, read> blends: array<u32>;
@group(1) @binding(5)
var<storage, read> opacities: array<f32>;

var<push_constant> layer_count: i32;

// Blend alpha straight colors
fn premultiplied_blend(bg: vec4f, fg: vec4f, cg: vec4f) -> vec4f {
    return clamp(vec4(
        cg.rgb * cg.a * bg.a + comp(fg.rgb, bg.a) + comp(bg.rgb, cg.a),
        stdalpha(bg.a, cg.a)
    ), vec4(0.0), vec4(1.0));
}

const MASK_NONE: u32 = 0xFFFFFFFFu;

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    // Premultiplied colors
    var bga = textureSample(composite, splr, in.bg_coords);

    for (var i: i32 = 0; i < layer_count; i++) {
        var maska = select(textureSample(textures, splr, in.fg_coords, i32(masks[i])).a, 1.0, masks[i] == MASK_NONE);
        var fga = textureSample(textures, splr, in.fg_coords, i32(layers[i])) * maska;

        // Short circuit
        // if (bga.a == 0.0) {
        //     bga = vec4(fga.rgb, min(fga.a, maska) * opacities[i]);
        //     continue;
        // } 
        //else if (fga.a == 0.0) {
        //     return bga;
        // }    

        var bg = vec4(clamp(bga.rgb / bga.a, vec3(0.0), vec3(1.0)), bga.a);
        var fg = vec4(clamp(fga.rgb / fga.a, vec3(0.0), vec3(1.0)), fga.a * opacities[i]);

        // Blend straight colors according to modes
        var final_pixel = vec3(0.0);
        switch (blends[i]) {
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
            case 13u: { final_pixel = color(bg.rgb, fg.rgb); }
            case 14u: { final_pixel = luminosity(bg.rgb, fg.rgb); }
            case 15u: { final_pixel = hue(bg.rgb, fg.rgb); }
            case 16u: { final_pixel = saturation(bg.rgb, fg.rgb); }
            case 17u: { final_pixel = soft_light(bg.rgb, fg.rgb); }
            case 19u: { final_pixel = darken(bg.rgb, fg.rgb); }
            case 20u: { final_pixel = hard_mix(bg.rgb, fg.rgb); }
            case 21u: { final_pixel = vivid_light(bg.rgb, fg.rgb); }
            case 22u: { final_pixel = linear_light(bg.rgb, fg.rgb); }
            case 23u: { final_pixel = pin_light(bg.rgb, fg.rgb); }
            case 24u: { final_pixel = lighter_color(bg.rgb, fg.rgb); }
            case 25u: { final_pixel = darker_color(bg.rgb, fg.rgb); }
            case 26u: { final_pixel = divide(bg.rgb, fg.rgb); }
            default: { final_pixel = normal(bg.rgb, fg.rgb); }
        }
        // Clamp to avoid unwanted behavior down the road
        final_pixel = clamp(final_pixel, vec3(0.0), vec3(1.0));

        // Compute final premultiplied colors
        bga = premultiplied_blend(bga, fga, vec4(final_pixel, fg.a));
    }
    return bga;
}
