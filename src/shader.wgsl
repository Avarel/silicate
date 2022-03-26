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

fn normal(dca: vec3<f32>, sca: vec3<f32>, _: f32, sa: f32) -> vec3<f32> {
    return sca + comp(dca, sa);
}

fn multiply(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return sca * dca + comp(sca, da) + comp(dca, sa);
}

fn screen(dca: vec3<f32>, sca: vec3<f32>, _: f32, _: f32) -> vec3<f32> {
    return sca + dca - sca * dca;
}

fn overlay_c(dca: f32, sca: f32, da: f32, sa: f32) -> f32 {
    if (dca * 2.0 <= da) {
        return sca * dca * 2.0;
    } else {
        return -2.0 * (da - dca) * (sa - sca) + sa * da;
    }
}


fn overlay(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return vec3<f32>(
        overlay_c(dca.r, sca.r, da, sa), 
        overlay_c(dca.g, sca.g, da, sa), 
        overlay_c(dca.b, sca.b, da, sa)
    ) + comp(sca, da) + comp(dca, sa);
}

fn darken(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return min(sca * da, dca * sa) + comp(sca, da) + comp(dca, sa);
}

fn lighten(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return max(sca * da, dca * sa) + comp(sca, da) + comp(dca, sa);
}

fn difference(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return sca + dca - 2.0 * min(sca * da, dca * sa);
}

fn exclusion(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return (sca * da + dca * sa - 2.0 * sca * dca) + comp(sca, da) + comp(dca, sa);
}

fn hard_light(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return overlay(sca, dca, sa, da);
}

fn color_dodge_c(dca: f32, sca: f32, da: f32, sa: f32) -> f32 {
    if (sca == sa && dca == 0.0) {
        return 0.0;
    } else if (sca == sa) {
        return sa * da;
    } else {
        return sa * da * min(1.0, dca/da * sa/(sa - sca));
    }
}

fn color_dodge(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return vec3<f32>(
        color_dodge_c(dca.r, sca.r, da, sa), 
        color_dodge_c(dca.g, sca.g, da, sa), 
        color_dodge_c(dca.b, sca.b, da, sa)
    ) + comp(sca, da) + comp(dca, sa);
}

// Half working
fn soft_light(dca: vec3<f32>, sca: vec3<f32>, da: f32, sa: f32) -> vec3<f32> {
    return mix(
        sqrt(dca) * (2.0 * sca - 1.0) + 2.0 * dca * (1.0 - sca), 
        2.0 * dca * sca + dca * dca * (1.0 - 2.0 * sca) + comp(sca, da) + comp(dca, sa),
        step(dca, vec3<f32>(0.5))
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
    fg.a = fg.a * ctx.opacity;

    fg = select(fg, vec4<f32>(0.0), fg.a == 0.0);

    var final_pixel = vec3<f32>(0.0);

    switch (ctx.blend) {
        case 1: {
            final_pixel = multiply(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 2: {
            final_pixel = screen(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 4: {
            final_pixel = lighten(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 5: {
            final_pixel = exclusion(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 6: {
            final_pixel = difference(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 9: {
            final_pixel = color_dodge(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 11: {
            final_pixel = overlay(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 12: {
            final_pixel = hard_light(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 17: {
            final_pixel = soft_light(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        case 19: {
            final_pixel = darken(bg.rgb, fg.rgb, bg.a, fg.a);
        }
        default: {
            final_pixel = normal(bg.rgb, fg.rgb, bg.a, fg.a);
        }
    }

    return vec4<f32>(final_pixel, bg.a + fg.a - bg.a * fg.a);
}
