#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_view_bindings::globals

struct WaterMaterial {
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    wave: vec4<f32>,
    foam: vec4<f32>,
    weather: vec4<f32>,
};

@group(2) @binding(0)
var<uniform> material: WaterMaterial;

fn saturate_color(c: vec3<f32>, amount: f32) -> vec3<f32> {
    let luma = dot(c, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(luma), c, amount);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let t = globals.time;
    let wp = in.world_position.xyz;

    let a = sin((wp.x + t * material.wave.y) * material.wave.x);
    let b = cos((wp.z - t * material.wave.y * 0.83) * material.wave.x * 1.24);
    let c = sin((wp.x + wp.z + t * material.wave.y * 0.42) * material.wave.x * 0.67);
    let wave_h = (a + b + c) / 3.0;

    let nx = cos((wp.x + t * material.wave.y) * material.wave.x) * material.wave.x;
    let nz = -sin((wp.z - t * material.wave.y * 0.83) * material.wave.x * 1.24) * material.wave.x;
    let n = normalize(vec3<f32>(-nx * material.wave.z, 1.0, -nz * material.wave.z));

    let sun_dir = normalize(vec3<f32>(-0.45, -1.0, -0.25));
    let sun = pow(max(dot(n, -sun_dir), 0.0), 34.0) * 0.8;

    let view_dir = normalize(view.world_position - wp);
    let fresnel = pow(1.0 - max(dot(n, view_dir), 0.0), 3.0);

    let depth_input = clamp(in.color.r, 0.0, 1.0);
    let depth_mix = clamp(depth_input * 0.95 + wave_h * 0.10, 0.0, 1.0);
    var col = mix(material.deep_color.rgb, material.shallow_color.rgb, depth_mix);

    let shoreline = smoothstep(0.0, 0.25, 1.0 - depth_input);
    let crest = smoothstep(0.74, 0.98, wave_h);
    let ripple = 0.5 + 0.5 * sin((wp.x + wp.z) * 0.23 + t * 2.6);
    let foam = max(shoreline * 1.1, crest * 0.7 + ripple * 0.35 * shoreline) * material.foam.x;
    col = col + vec3<f32>(foam);
    let sky_reflect = mix(vec3<f32>(0.48, 0.66, 0.92), vec3<f32>(0.88, 0.95, 1.0), clamp(n.y, 0.0, 1.0));
    let glint = pow(max(dot(reflect(view_dir, n), -sun_dir), 0.0), 72.0) * 0.85;
    col = mix(col, sky_reflect, fresnel * (0.52 + material.weather.x * 0.22));
    col += vec3<f32>(sun + glint);
    col *= 1.0 - material.weather.x * 0.08;

    let dist = distance(wp, view.world_position);
    let fog_t = smoothstep(240.0, 860.0, dist);
    col = mix(col, vec3<f32>(0.55, 0.8, 0.98), fog_t * 0.72);
    col = saturate_color(col, 1.16);

    let alpha = mix(material.deep_color.a, 0.99, fresnel * 0.78 + shoreline * 0.12);
    return vec4<f32>(col, alpha);
}
