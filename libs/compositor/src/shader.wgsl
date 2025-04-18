// Vertex shader ///////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
// Nothing special about this section. It gets fed vertices in a triangle strip
// configuration to draw a square on the texture.

struct CanvasTiling {
    height: u32,
    width: u32,
    cols: u32,
    rows: u32,
    tile_size: u32,
    flipped: u32,
};

@group(0) @binding(0)
var<uniform> canvas: CanvasTiling;

struct VertexInput {
    @location(0) position: vec2f,
    @location(1) coords: vec2f,
};

struct TileInstance {
    @location(2) col: u32,
    @location(3) row: u32
}

struct VertexOutput {
    @builtin(position) position: vec4f,
    @location(0) coords: vec2f,
    @location(1) chunk_index: u32,
};

@vertex
fn vs_main(
    model: VertexInput,
    tile: TileInstance
) -> VertexOutput {
    let tile_coords = vec2f(f32(tile.col), f32(tile.row));
    let canvas_grid = vec2f(f32(canvas.cols), f32(canvas.rows));
    let canvas_dim = vec2f(f32(canvas.width), f32(canvas.height));

    let flipped_horizontally = (canvas.flipped >> 1 & 1) != 0;
    let flipped_vertically = (canvas.flipped & 1) != 0;

    let scale = canvas_grid * f32(canvas.tile_size) / canvas_dim;
    let pos = (model.position + tile_coords) / canvas_grid;
    let normalized_pos = select(pos * scale, 1.0 - pos * scale, vec2(flipped_horizontally, flipped_vertically)) * 2.0 - 1.0;

    var out: VertexOutput;
    out.position = vec4(normalized_pos, 0.0, 1.0);
    out.coords = model.coords;
    out.chunk_index = tile.row * canvas.cols + tile.col;
    return out;
}

// Blening cod ///////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////

// HSL Blending Modes //////////////////////////////////////////////////////////
// [PDF Blend Modes: Addendum]
// [KHR_blend_equation_advanced]
fn rgb2hsv(c: vec3f) -> vec3f {
    let K = vec4f(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4f(c.bg, K.wz), vec4f(c.gb, K.xy), step(c.b, c.g));
    let q = mix(vec4f(p.xyw, c.r), vec4f(c.r, p.yzx), step(p.x, c.r));

    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec3f(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

fn hsv2rgb(c: vec3f) -> vec3f {
    let K = vec4f(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, saturate(p - K.xxx), c.y);
}

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
    return saturate(z);
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

struct AtlasData {
    cols: u32,
    rows: u32
}

struct LayerData {
    opacity: f32,
    blend: u32,
    clipped: u32,
    hidden: u32,
};

struct ChunkData {
    atlas_coords: u32,
    clip_atlas_index: u32,
    layer_index: u32,
};

struct SegmentData {
    start: u32,
    end: u32,
}

@group(1) @binding(0)
var splr: sampler;
@group(2) @binding(0)
var<uniform> atlas: AtlasData;
@group(2) @binding(1)
var atlas_texture: texture_2d_array<f32>;
@group(2) @binding(2)
var<storage, read> chunks: array<ChunkData>;
@group(2) @binding(3)
var<storage, read> layers: array<LayerData>;
@group(2) @binding(4)
var<storage, read> segments: array<SegmentData>;

// Blend alpha straight colors
fn premultiplied_blend(bg: vec4f, fg: vec4f, cg: vec4f) -> vec4f {
    return saturate(vec4(
        cg.rgb * cg.a * bg.a + comp(fg.rgb, bg.a) + comp(bg.rgb, cg.a),
        stdalpha(bg.a, cg.a)
    ));
}

fn atlas_coords(atlas_coords: u32) -> vec3u {
    return vec3u(
        atlas_coords % atlas.cols,
        atlas_coords / atlas.cols % atlas.rows,
        atlas_coords / (atlas.cols * atlas.rows)
    );
}

fn sample_atlas_texture(atlas_index: u32, coords: vec2f) -> vec4f {
    let a_grid = vec2f(f32(atlas.cols), f32(atlas.rows));
    let a_coords = atlas_coords(atlas_index);
    let uv = (vec2f(a_coords.xy) + coords) / a_grid;
    return textureSample(atlas_texture, splr, uv, a_coords.z);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4f {
    var bg = vec4(0.0);

    let segment = segments[in.chunk_index];

    for (var i: u32 = segment.start; i < segment.end; i++) {
        let chunk = chunks[i];

        let layer = layers[chunk.layer_index];

        if (layer.hidden != 0) {
            continue;
        }

        var clipa = select(sample_atlas_texture(chunk.clip_atlas_index, in.coords).a, 1.0, layer.clipped == 0);
        var fg = sample_atlas_texture(chunk.atlas_coords, in.coords) * clipa;

        // Blend straight colors according to modes
        var final_pixel = vec3(0.0);
        switch (layer.blend) {
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
        final_pixel = saturate(final_pixel);

        // Compute final premultiplied colors
        bg = premultiplied_blend(bg, fg, vec4(final_pixel, fg.a * layer.opacity));
    }
    return bg;
}
