#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_view_bindings::globals

struct VoxelMaterial {
    sun_dir_and_strength: vec4<f32>,
    fog_color: vec4<f32>,
    fog_distances: vec4<f32>,
    ao: vec4<f32>,
    weather: vec4<f32>,
};

@group(2) @binding(0)
var<uniform> material: VoxelMaterial;
@group(2) @binding(1)
var atlas_tex: texture_2d<f32>;
@group(2) @binding(2)
var atlas_sampler: sampler;

fn hash13(p: vec3<f32>) -> f32 {
    let h = dot(p, vec3<f32>(127.1, 311.7, 74.7));
    return fract(sin(h) * 43758.5453);
}

fn hash12(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(127.1, 311.7))) * 43758.5453);
}

fn face_uv(world_pos: vec3<f32>, n: vec3<f32>) -> vec2<f32> {
    let an = abs(n);
    if an.x > an.y && an.x > an.z {
        return fract(world_pos.zy);
    } else if an.y > an.z {
        return fract(world_pos.xz);
    }
    return fract(world_pos.xy);
}

fn saturate_color(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(luma), c, amount);
}

fn contrast_color(c: vec3<f32>, amount: f32) -> vec3<f32> {
    return (c - vec3<f32>(0.5)) * amount + vec3<f32>(0.5);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let sun_dir = normalize(material.sun_dir_and_strength.xyz);
    let block_cell = floor(in.world_position.xyz - n * 0.05);
    let packed = in.color.a;
    let tex_id = floor(packed + 0.0001);
    let vert_ao = clamp(fract(packed), 0.45, 0.999);

    let cols = 9.0;
    let rows = 10.0;
    let tile_col = tex_id - floor(tex_id / cols) * cols;
    let tile_row = floor(tex_id / cols);
    let tile_size = vec2<f32>(1.0 / cols, 1.0 / rows);
    // Inset UVs slightly so distant sampling does not bleed into adjacent atlas tiles.
    let inset_px = 1.0;
    let uv_inset = vec2<f32>(inset_px / 128.0, inset_px / 128.0);
    let uv_local = uv_inset + fract(in.uv) * (vec2<f32>(1.0, 1.0) - uv_inset * 2.0);
    let atlas_uv = vec2<f32>(
        (tile_col + uv_local.x) * tile_size.x,
        (tile_row + uv_local.y) * tile_size.y
    );

    let texel = textureSample(atlas_tex, atlas_sampler, atlas_uv).rgb;
    let block_tint = in.color.rgb;

    let tint_noise = hash13(block_cell);
    let macro_noise = hash12(floor(in.world_position.xz * 0.08));
    let tint = 0.88 + tint_noise * 0.10 + macro_noise * 0.08;
    let base = texel * block_tint * tint;

    let sun = pow(max(dot(n, -sun_dir), 0.0), 1.35);
    let hemi = clamp(n.y * 0.5 + 0.5, 0.0, 1.0);
    let ao = clamp((1.0 - material.ao.x * (1.0 - hemi)) * vert_ao, material.ao.y, 1.0);

    let sky_ambient = mix(vec3<f32>(0.10, 0.10, 0.11), vec3<f32>(0.46, 0.56, 0.70), hemi);
    let sun_light = vec3<f32>(1.0, 0.97, 0.90) * (sun * material.sun_dir_and_strength.w * 1.15);
    var lit = base * (sky_ambient + sun_light);
    lit *= ao;

    // Moving cloud shadow projection onto terrain.
    let cloud_uv = in.world_position.xz * material.weather.y + vec2<f32>(globals.time * material.weather.z, globals.time * material.weather.z * 0.71);
    let cloud = hash12(floor(cloud_uv * 2.0)) * 0.65 + hash12(floor(cloud_uv * 5.0)) * 0.35;
    let cloud_shadow = mix(1.0 - material.weather.x, 1.0, smoothstep(0.45, 0.75, cloud));
    lit *= cloud_shadow;
    lit *= 1.0 - material.weather.w * 0.18;

    let uv = face_uv(in.world_position.xyz, n);
    let edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let edge = 1.0 - smoothstep(0.018, 0.08, edge_dist);
    lit *= 1.0 - edge * 0.17;

    let height_t = clamp((in.world_position.y - 16.0) / 96.0, 0.0, 1.0);
    lit = mix(lit * 0.92, lit * vec3(1.07, 1.10, 1.12), height_t);
    lit = saturate_color(lit, 1.22);
    lit = contrast_color(lit, 1.10);

    let dist = distance(in.world_position.xyz, view.world_position);
    let fog_dist_t = smoothstep(material.fog_distances.x, material.fog_distances.y, dist);
    let height_fog = smoothstep(96.0, -24.0, in.world_position.y) * (0.35 + material.weather.w * 0.45);
    let fog_t = clamp(max(fog_dist_t, height_fog), 0.0, 1.0);
    let fogged = mix(lit, material.fog_color.rgb, fog_t * 0.78);
    let color = saturate_color(fogged, 1.08);

    return vec4<f32>(color, 1.0);
}
