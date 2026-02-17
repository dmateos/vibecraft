#import bevy_pbr::forward_io::VertexOutput
#import bevy_pbr::mesh_view_bindings::view
#import bevy_pbr::mesh_view_bindings::globals

struct WaterMaterial {
    shallow_color: vec4<f32>,
    deep_color: vec4<f32>,
    wave: vec4<f32>,
    foam: vec4<f32>,
};

@group(2) @binding(0)
var<uniform> material: WaterMaterial;

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
    let sun = pow(max(dot(n, -sun_dir), 0.0), 24.0) * 0.4;

    let view_dir = normalize(view.world_position - wp);
    let fresnel = pow(1.0 - max(dot(n, view_dir), 0.0), 3.0);

    let depth_mix = clamp(0.5 + wave_h * 0.5, 0.0, 1.0);
    var col = mix(material.deep_color.rgb, material.shallow_color.rgb, depth_mix);

    let foam = smoothstep(0.72, 0.97, wave_h) * material.foam.x;
    col = col + vec3<f32>(foam);
    col = mix(col, vec3<f32>(0.82, 0.92, 1.0), fresnel * 0.45);
    col += vec3<f32>(sun);

    let dist = distance(wp, view.world_position);
    let fog_t = smoothstep(340.0, 960.0, dist);
    col = mix(col, vec3<f32>(0.55, 0.8, 0.98), fog_t);

    let alpha = mix(material.deep_color.a, 0.98, fresnel * 0.6);
    return vec4<f32>(col, alpha);
}
