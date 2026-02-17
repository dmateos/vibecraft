#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view

struct VoxelMaterial {
    sun_dir_and_strength: vec4<f32>,
    fog_color: vec4<f32>,
    fog_distances: vec4<f32>,
    ao: vec4<f32>,
};

@group(2) @binding(0)
var<uniform> material: VoxelMaterial;

fn hash13(p: vec3<f32>) -> f32 {
    let h = dot(p, vec3<f32>(127.1, 311.7, 74.7));
    return fract(sin(h) * 43758.5453);
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

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let n = normalize(in.world_normal);
    let sun_dir = normalize(material.sun_dir_and_strength.xyz);
    let block_cell = floor(in.world_position.xyz - n * 0.05);

    let tint_noise = hash13(block_cell);
    let tint = 0.92 + tint_noise * 0.16;
    let base = in.color.rgb * tint;

    let sun = pow(max(dot(n, -sun_dir), 0.0), 1.35);
    let hemi = clamp(n.y * 0.5 + 0.5, 0.0, 1.0);
    let ao = clamp(1.0 - material.ao.x * (1.0 - hemi), material.ao.y, 1.0);

    let sky_ambient = mix(vec3<f32>(0.14, 0.13, 0.13), vec3<f32>(0.44, 0.50, 0.60), hemi);
    let sun_light = vec3<f32>(1.0, 0.97, 0.92) * (sun * material.sun_dir_and_strength.w);
    var lit = base * (sky_ambient + sun_light);
    lit *= ao;

    let uv = face_uv(in.world_position.xyz, n);
    let edge_dist = min(min(uv.x, 1.0 - uv.x), min(uv.y, 1.0 - uv.y));
    let edge = 1.0 - smoothstep(0.018, 0.08, edge_dist);
    lit *= 1.0 - edge * 0.17;

    let height_t = clamp((in.world_position.y - 16.0) / 96.0, 0.0, 1.0);
    lit = mix(lit * 0.92, lit * vec3(1.07, 1.10, 1.12), height_t);

    let dist = distance(in.world_position.xyz, view.world_position);
    let fog_t = smoothstep(material.fog_distances.x, material.fog_distances.y, dist);
    let color = mix(lit, material.fog_color.rgb, fog_t);

    return vec4<f32>(color, 1.0);
}
