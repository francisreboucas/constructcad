struct Uniforms {
    view_proj: mat4x4<f32>,
    light_dir: vec4<f32>,
    /// x: 1 to leave faces unlit (hidden line, consistent colors).
    style: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: Uniforms;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
    /// Surface pattern over the face (textures style): rgb its color, w its kind.
    @location(3) fg: vec4<f32>,
    @location(4) bg: vec4<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
    @location(3) fg: vec4<f32>,
    @location(4) bg: vec4<f32>,
};

// The target is a plain (non-sRGB) texture shown by egui, which expects
// gamma-encoded values: shade in linear space, then encode.
fn gamma_from_linear(rgb: vec3<f32>) -> vec3<f32> {
    let cutoff = rgb < vec3<f32>(0.0031308);
    let lower = rgb * vec3<f32>(12.92);
    let higher = vec3<f32>(1.055) * pow(rgb, vec3<f32>(1.0 / 2.4)) - vec3<f32>(0.055);
    return select(higher, lower, cutoff);
}

@vertex
fn vs_main(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = u.view_proj * vec4<f32>(v.pos, 1.0);
    o.normal = v.normal;
    o.color = v.color;
    o.world = v.pos;
    o.fg = v.fg;
    o.bg = v.bg;
    return o;
}

// How much of a line repeating every `spacing` covers coordinate `v`, with
// `w` the coordinate's change across a pixel (antialiased).
fn lines(v: f32, spacing: f32, w: f32) -> f32 {
    let d = abs(fract(v / spacing + 0.5) - 0.5) * spacing;
    return 1.0 - smoothstep(0.5 * w, 1.5 * w, d);
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

// Scattered dots on a grid of `cell` meters, about `share` of the cells.
fn dots(uv: vec2<f32>, cell: f32, share: f32, w: f32) -> f32 {
    let c = floor(uv / cell);
    let center = (c + vec2<f32>(0.3 + 0.4 * hash(c + 7.0), 0.3 + 0.4 * hash(c + 13.0))) * cell;
    let r = 0.12 * cell;
    let inside = 1.0 - smoothstep(r - w, r + w, length(uv - center));
    return inside * step(1.0 - share, hash(c));
}

// Coverage of pattern `kind` (the order of `cc_core::Pattern`) at `uv`, in meters.
fn pattern(kind: f32, uv: vec2<f32>, w: f32) -> f32 {
    let k = i32(round(kind));
    let s = 0.15;
    var c = 0.0;
    if k == 1 {
        c = 1.0;
    } else if k == 2 {
        c = lines(uv.x + uv.y, s, w * 1.4);
    } else if k == 3 {
        c = lines(uv.x - uv.y, s, w * 1.4);
    } else if k == 4 {
        c = max(lines(uv.x + uv.y, s, w * 1.4), lines(uv.x - uv.y, s, w * 1.4));
    } else if k == 5 {
        c = lines(uv.y, s, w);
    } else if k == 6 {
        c = lines(uv.x, s, w);
    } else if k == 7 {
        c = max(lines(uv.x, 0.3, w), lines(uv.y, 0.3, w));
    } else if k == 8 {
        // Running bond: courses of 7.5 cm, bricks of 22.5 cm, every other course shifted half a brick.
        let row = floor(uv.y / 0.075);
        let odd = row - 2.0 * floor(row * 0.5);
        c = max(lines(uv.y, 0.075, w), lines(uv.x + odd * 0.1125, 0.225, w));
    } else if k == 9 {
        c = max(dots(uv, 0.06, 0.35, w), dots(uv + 0.031, 0.09, 0.2, w));
    } else if k == 10 {
        c = dots(uv, 0.025, 0.5, w);
    } else if k == 11 {
        // Batts: a zig-zag across the layer.
        c = lines(uv.y + abs(fract(uv.x / 0.1) - 0.5) * 0.1, 0.1, w * 1.4);
    }
    return c;
}

@fragment
fn fs_face(i: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(i.normal);
    // The face's own plane coordinates: the axes it faces least.
    let a = abs(n);
    let top = a.z >= a.x && a.z >= a.y;
    let side_x = !top && a.x >= a.y;
    let uv = select(select(i.world.xz, i.world.yz, side_x), i.world.xy, top);
    // Derivatives in uniform control flow, before any branch.
    let fw = fwidth(uv);
    let w = max(max(fw.x, fw.y), 1e-5);
    var base = i.color.rgb;
    if i.bg.w > 0.5 {
        base = mix(base, i.bg.rgb, pattern(i.bg.w, uv, w));
    }
    if i.fg.w > 0.5 {
        base = mix(base, i.fg.rgb, pattern(i.fg.w, uv, w));
    }
    let diffuse = max(dot(n, normalize(u.light_dir.xyz)), 0.0);
    // Hemispheric ambient keeps faces turned away from the sun readable.
    let sky = 0.5 + 0.5 * n.z;
    let lit = 0.30 + 0.60 * diffuse + 0.25 * sky;
    let k = select(lit, 1.0, u.style.x > 0.5);
    return vec4<f32>(gamma_from_linear(base * k), i.color.a);
}

@fragment
fn fs_edge(i: VsOut) -> @location(0) vec4<f32> {
    return vec4<f32>(gamma_from_linear(i.color.rgb), 1.0);
}
