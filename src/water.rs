#![allow(dead_code)]

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::pbr::{Material, MaterialPlugin, NotShadowCaster};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::mesh::Indices;
use bevy::render::render_asset::RenderAssetUsages;
use bevy::render::render_resource::{AsBindGroup, PrimitiveTopology, ShaderRef, ShaderType};

use crate::config::{CHUNK_SIZE, SEA_LEVEL, VIEW_DISTANCE_CHUNKS, WORLD_HEIGHT};
use crate::generation::PromptInputState;
use crate::interact::LocalBlockEditEvent;
use crate::player::FlyCam;
use crate::world::{chunk_distance_sq, div_floor, get_block_world, Block, Chunk, VoxelWorld};

const MAX_WATER_CHUNKS_PER_TICK: usize = 24;
const MAX_WATER_SIM_CELLS_PER_TICK: usize = 180;
const MAX_WATER_REMESH_PER_TICK: usize = 2;

#[derive(Clone, Copy, Debug, ShaderType)]
pub struct WaterMaterialParams {
    pub shallow_color: Vec4,
    pub deep_color: Vec4,
    pub wave: Vec4,
    pub foam: Vec4,
    pub weather: Vec4,
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct WaterSurfaceMaterial {
    #[uniform(0)]
    pub params: WaterMaterialParams,
}

impl Material for WaterSurfaceMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/water_material.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Blend
    }
}

#[derive(Resource)]
pub struct WaterMaterial(pub Handle<WaterSurfaceMaterial>);

#[derive(Clone)]
struct WaterRender {
    entity: Entity,
    mesh: Handle<Mesh>,
}

#[derive(Resource, Default)]
pub struct LoadedWater {
    entries: HashMap<IVec2, WaterRender>,
}

impl LoadedWater {
    pub fn clear_and_despawn(&mut self, commands: &mut Commands) {
        let items: Vec<WaterRender> = self.entries.values().cloned().collect();
        self.entries.clear();
        for item in items {
            commands.entity(item.entity).despawn_recursive();
        }
    }
}

#[derive(Resource)]
pub struct WaterStreamTimer(pub Timer);

#[derive(Resource, Debug, Clone, Copy)]
pub struct WaterPhysicsConfig {
    pub enabled: bool,
}

impl Default for WaterPhysicsConfig {
    fn default() -> Self {
        Self { enabled: false }
    }
}

#[derive(Resource, Default)]
pub struct WaterFlowSim {
    levels: HashMap<IVec3, u8>,
    levels_by_chunk: HashMap<IVec2, HashSet<IVec3>>,
    queued: HashSet<IVec3>,
    queue: VecDeque<IVec3>,
    dirty_chunks: HashSet<IVec2>,
    source_masks: HashMap<IVec2, Vec<u8>>,
}

impl WaterFlowSim {
    pub fn clear(&mut self) {
        self.levels.clear();
        self.levels_by_chunk.clear();
        self.queued.clear();
        self.queue.clear();
        self.dirty_chunks.clear();
        self.source_masks.clear();
    }

    fn enqueue(&mut self, cell: IVec3) {
        if !(0..WORLD_HEIGHT as i32).contains(&cell.y) {
            return;
        }
        if self.queued.insert(cell) {
            self.queue.push_back(cell);
        }
    }

    fn mark_chunk_dirty(&mut self, chunk: IVec2) {
        self.dirty_chunks.insert(chunk);
        self.dirty_chunks.insert(chunk + IVec2::new(1, 0));
        self.dirty_chunks.insert(chunk + IVec2::new(-1, 0));
        self.dirty_chunks.insert(chunk + IVec2::new(0, 1));
        self.dirty_chunks.insert(chunk + IVec2::new(0, -1));
    }

    fn mark_cell_dirty(&mut self, cell: IVec3) {
        let chunk = IVec2::new(
            div_floor(cell.x, CHUNK_SIZE as i32),
            div_floor(cell.z, CHUNK_SIZE as i32),
        );
        self.mark_chunk_dirty(chunk);
    }

    fn queue_neighbors(&mut self, cell: IVec3) {
        let offsets = [
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(-1, 0, 0),
            IVec3::new(0, 0, 1),
            IVec3::new(0, 0, -1),
            IVec3::new(0, 1, 0),
            IVec3::new(0, -1, 0),
            IVec3::new(1, 1, 0),
            IVec3::new(-1, 1, 0),
            IVec3::new(0, 1, 1),
            IVec3::new(0, 1, -1),
            IVec3::new(1, -1, 0),
            IVec3::new(-1, -1, 0),
            IVec3::new(0, -1, 1),
            IVec3::new(0, -1, -1),
        ];

        for o in offsets {
            self.enqueue(cell + o);
        }
    }

}

pub fn water_material_plugin() -> MaterialPlugin<WaterSurfaceMaterial> {
    MaterialPlugin::<WaterSurfaceMaterial>::default()
}

pub fn setup_water(
    mut commands: Commands,
    mut materials: ResMut<Assets<WaterSurfaceMaterial>>,
) {
    let material = materials.add(WaterSurfaceMaterial {
        params: WaterMaterialParams {
            shallow_color: Vec4::new(0.10, 0.58, 0.88, 0.90),
            deep_color: Vec4::new(0.01, 0.12, 0.36, 0.96),
            wave: Vec4::new(0.080, 2.7, 1.15, 0.5),
            foam: Vec4::new(0.26, 0.0, 0.0, 0.0),
            weather: Vec4::ZERO,
        },
    });

    commands.insert_resource(WaterMaterial(material));
}

pub fn queue_water_updates_from_block_edits(
    cfg: Res<WaterPhysicsConfig>,
    world: Res<VoxelWorld>,
    mut edits: EventReader<LocalBlockEditEvent>,
    mut sim: ResMut<WaterFlowSim>,
) {
    if !cfg.enabled {
        edits.clear();
        return;
    }
    for edit in edits.read() {
        let center = IVec3::new(edit.x, edit.y, edit.z);
        let center_chunk = IVec2::new(
            div_floor(center.x, CHUNK_SIZE as i32),
            div_floor(center.z, CHUNK_SIZE as i32),
        );
        // Fast-path: most edits are nowhere near water, so skip all water work.
        let near_dynamic = has_nearby_dynamic_water(center, &sim);
        let near_sea_band = (center.y - SEA_LEVEL).abs() <= 2;
        if !near_dynamic && !near_sea_band {
            continue;
        }
        ensure_source_mask_for_chunk(
            &mut sim,
            &world.chunks,
            center_chunk,
        );
        if !has_nearby_water(center, &world.chunks, &sim) {
            continue;
        }

        let offsets = [
            IVec3::new(0, 0, 0),
            IVec3::new(1, 0, 0),
            IVec3::new(-1, 0, 0),
            IVec3::new(0, 0, 1),
            IVec3::new(0, 0, -1),
            IVec3::new(0, 1, 0),
            IVec3::new(0, -1, 0),
            IVec3::new(1, 1, 0),
            IVec3::new(-1, 1, 0),
            IVec3::new(0, 1, 1),
            IVec3::new(0, 1, -1),
            IVec3::new(1, -1, 0),
            IVec3::new(-1, -1, 0),
            IVec3::new(0, -1, 1),
            IVec3::new(0, -1, -1),
            IVec3::new(2, 0, 0),
            IVec3::new(-2, 0, 0),
            IVec3::new(0, 0, 2),
            IVec3::new(0, 0, -2),
        ];
        for o in offsets {
            sim.enqueue(center + o);
        }

        sim.mark_cell_dirty(center);
    }
}

pub fn tick_water_simulation(
    cfg: Res<WaterPhysicsConfig>,
    world: Res<VoxelWorld>,
    mut sim: ResMut<WaterFlowSim>,
) {
    if !cfg.enabled {
        return;
    }
    let mut processed = 0usize;

    while processed < MAX_WATER_SIM_CELLS_PER_TICK {
        let Some(cell) = sim.queue.pop_front() else {
            break;
        };
        sim.queued.remove(&cell);
        processed += 1;

        if !(0..WORLD_HEIGHT as i32).contains(&cell.y) {
            continue;
        }

        let old = sim.levels.get(&cell).copied().unwrap_or(0);
        let chunk = IVec2::new(
            div_floor(cell.x, CHUNK_SIZE as i32),
            div_floor(cell.z, CHUNK_SIZE as i32),
        );
        if is_source_cell(cell, &world.chunks, &sim) {
            if old != 0 {
                sim.levels.remove(&cell);
                if let Some(set) = sim.levels_by_chunk.get_mut(&chunk) {
                    set.remove(&cell);
                    if set.is_empty() {
                        sim.levels_by_chunk.remove(&chunk);
                    }
                }
                sim.mark_cell_dirty(cell);
                sim.queue_neighbors(cell);
            }
            continue;
        }
        let new = compute_water_level(cell, &world, &mut sim);

        if new == old {
            continue;
        }

        if new == 0 {
            sim.levels.remove(&cell);
            if let Some(set) = sim.levels_by_chunk.get_mut(&chunk) {
                set.remove(&cell);
                if set.is_empty() {
                    sim.levels_by_chunk.remove(&chunk);
                }
            }
        } else {
            sim.levels.insert(cell, new);
            sim.levels_by_chunk.entry(chunk).or_default().insert(cell);
        }

        sim.mark_cell_dirty(cell);
        sim.queue_neighbors(cell);
    }
}

pub fn refresh_dirty_water_meshes(
    cfg: Res<WaterPhysicsConfig>,
    mut sim: ResMut<WaterFlowSim>,
    world: Res<VoxelWorld>,
    loaded: Res<LoadedWater>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !cfg.enabled {
        return;
    }
    if sim.dirty_chunks.is_empty() {
        return;
    }

    let mut dirty: Vec<IVec2> = sim.dirty_chunks.drain().collect();
    dirty.sort_unstable_by_key(|pos| (pos.x, pos.y));

    let mut updated = 0usize;
    for pos in dirty {
        if updated >= MAX_WATER_REMESH_PER_TICK {
            sim.dirty_chunks.insert(pos);
            continue;
        }

        let Some(render) = loaded.entries.get(&pos) else {
            continue;
        };
        let Some(mesh) = meshes.get_mut(&render.mesh) else {
            continue;
        };

        ensure_source_mask_for_chunk(&mut sim, &world.chunks, pos);
        *mesh = build_water_mesh_for_chunk(pos, &world.chunks, &sim);
        updated += 1;
    }
}

pub fn stream_water_around_camera(
    mut commands: Commands,
    time: Res<Time>,
    mut timer: ResMut<WaterStreamTimer>,
    mut loaded: ResMut<LoadedWater>,
    world: Res<VoxelWorld>,
    mut sim: ResMut<WaterFlowSim>,
    mut meshes: ResMut<Assets<Mesh>>,
    water_material: Res<WaterMaterial>,
    cam_q: Query<&Transform, With<FlyCam>>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let cam_chunk = IVec2::new(
        div_floor(cam.translation.x.floor() as i32, CHUNK_SIZE as i32),
        div_floor(cam.translation.z.floor() as i32, CHUNK_SIZE as i32),
    );

    let mut desired = HashSet::new();
    for dz in -VIEW_DISTANCE_CHUNKS..=VIEW_DISTANCE_CHUNKS {
        for dx in -VIEW_DISTANCE_CHUNKS..=VIEW_DISTANCE_CHUNKS {
            if dx * dx + dz * dz > VIEW_DISTANCE_CHUNKS * VIEW_DISTANCE_CHUNKS {
                continue;
            }
            desired.insert(IVec2::new(cam_chunk.x + dx, cam_chunk.y + dz));
        }
    }

    let loaded_positions: Vec<IVec2> = loaded.entries.keys().copied().collect();
    for pos in loaded_positions {
        if !desired.contains(&pos)
            && let Some(entry) = loaded.entries.remove(&pos)
        {
            commands.entity(entry.entity).despawn_recursive();
        }
    }

    let mut to_spawn: Vec<IVec2> = desired
        .iter()
        .copied()
        .filter(|pos| !loaded.entries.contains_key(pos))
        .filter(|pos| world.chunks.contains_key(pos))
        .collect();
    to_spawn.sort_by_key(|pos| chunk_distance_sq(*pos, cam_chunk));
    to_spawn.truncate(MAX_WATER_CHUNKS_PER_TICK);

    for pos in to_spawn {
        ensure_source_mask_for_chunk(&mut sim, &world.chunks, pos);
        let mesh = meshes.add(build_water_mesh_for_chunk(pos, &world.chunks, &sim));
        let translation = Vec3::new(
            (pos.x * CHUNK_SIZE as i32) as f32,
            0.0,
            (pos.y * CHUNK_SIZE as i32) as f32,
        );

        let entity = commands
            .spawn((
                MaterialMeshBundle::<WaterSurfaceMaterial> {
                    mesh: mesh.clone(),
                    material: water_material.0.clone(),
                    transform: Transform::from_translation(translation),
                    ..default()
                },
                NotShadowCaster,
            ))
            .id();

        loaded.entries.insert(pos, WaterRender { entity, mesh });
    }
}

fn build_water_mesh_for_chunk(pos: IVec2, chunks: &HashMap<IVec2, Chunk>, sim: &WaterFlowSim) -> Mesh {
    let base_x = pos.x * CHUNK_SIZE as i32;
    let base_z = pos.y * CHUNK_SIZE as i32;

    let mut positions = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE * 4);
    let mut normals = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE * 4);
    let mut colors = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE * 4);
    let mut indices = Vec::with_capacity(CHUNK_SIZE * CHUNK_SIZE * 6);

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let wx = base_x + x as i32;
            let wz = base_z + z as i32;
            let cell = IVec3::new(wx, SEA_LEVEL, wz);
            if !is_source_cell(cell, chunks, sim) {
                continue;
            }

            if level_at(IVec3::new(wx, SEA_LEVEL + 1, wz), chunks, sim) > 0 {
                continue;
            }

            let surface = find_surface_y(chunks, wx, wz) as f32;
            let depth = ((SEA_LEVEL as f32 - surface) / 22.0).clamp(0.0, 1.0);
            let y = SEA_LEVEL as f32 + 0.02;
            push_top_quad(
                &mut positions,
                &mut normals,
                &mut colors,
                &mut indices,
                x as f32,
                y,
                z as f32,
                depth,
            );
        }
    }

    if let Some(cells) = sim.levels_by_chunk.get(&pos) {
        for &cell in cells {
            let level = sim.levels.get(&cell).copied().unwrap_or(0);
            if level == 0 {
                continue;
            }
        if is_solid_block(chunks, cell.x, cell.y, cell.z) {
            continue;
        }
        if level_at(cell + IVec3::Y, chunks, sim) > 0 {
            continue;
        }

        let lx = (cell.x - base_x) as f32;
        let lz = (cell.z - base_z) as f32;
        let y = cell.y as f32 + water_height(level);
        let depth = ((SEA_LEVEL - cell.y).max(0) as f32 / 28.0).clamp(0.15, 1.0);
        push_top_quad(
            &mut positions,
            &mut normals,
            &mut colors,
            &mut indices,
            lx,
            y,
            lz,
            depth,
        );
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    mesh.insert_indices(Indices::U32(indices));
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh
}

fn push_top_quad(
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    colors: &mut Vec<[f32; 4]>,
    indices: &mut Vec<u32>,
    x: f32,
    y: f32,
    z: f32,
    depth: f32,
) {
    let start = positions.len() as u32;
    positions.extend_from_slice(&[
        [x, y, z],
        [x + 1.0, y, z],
        [x + 1.0, y, z + 1.0],
        [x, y, z + 1.0],
    ]);
    normals.extend_from_slice(&[[0.0, 1.0, 0.0]; 4]);
    colors.extend_from_slice(&[[depth, 0.0, 0.0, 1.0]; 4]);
    indices.extend_from_slice(&[start, start + 2, start + 1, start, start + 3, start + 2]);
}

fn compute_water_level(cell: IVec3, world: &VoxelWorld, sim: &mut WaterFlowSim) -> u8 {
    if is_solid_block(&world.chunks, cell.x, cell.y, cell.z) {
        return 0;
    }

    if is_source_cell(cell, &world.chunks, sim) {
        return 8;
    }

    let above = level_at(cell + IVec3::Y, &world.chunks, sim);
    if above > 0 {
        return 8;
    }

    let below = cell - IVec3::Y;
    let below_solid = below.y < 0 || is_solid_block(&world.chunks, below.x, below.y, below.z);
    if !below_solid {
        return 0;
    }

    let mut best = 0u8;
    for side in [
        IVec3::new(1, 0, 0),
        IVec3::new(-1, 0, 0),
        IVec3::new(0, 0, 1),
        IVec3::new(0, 0, -1),
    ] {
        let v = level_at(cell + side, &world.chunks, sim);
        if v > 1 {
            best = best.max(v - 1);
        }
    }

    best
}

fn level_at(cell: IVec3, chunks: &HashMap<IVec2, Chunk>, sim: &WaterFlowSim) -> u8 {
    if cell.y < 0 || cell.y >= WORLD_HEIGHT as i32 {
        return 0;
    }
    if is_source_cell(cell, chunks, sim) {
        return 8;
    }
    sim.levels.get(&cell).copied().unwrap_or(0)
}

fn is_source_cell(cell: IVec3, chunks: &HashMap<IVec2, Chunk>, sim: &WaterFlowSim) -> bool {
    if cell.y != SEA_LEVEL {
        return false;
    }
    if is_solid_block(chunks, cell.x, cell.y, cell.z) {
        return false;
    }

    let chunk_pos = IVec2::new(
        div_floor(cell.x, CHUNK_SIZE as i32),
        div_floor(cell.z, CHUNK_SIZE as i32),
    );
    let Some(mask) = sim.source_masks.get(&chunk_pos) else {
        return false;
    };

    let lx = (cell.x - chunk_pos.x * CHUNK_SIZE as i32) as usize;
    let lz = (cell.z - chunk_pos.y * CHUNK_SIZE as i32) as usize;
    mask[lx + lz * CHUNK_SIZE] != 0
}

fn rebuild_source_mask_for_chunk(pos: IVec2, chunks: &HashMap<IVec2, Chunk>) -> Vec<u8> {
    let base_x = pos.x * CHUNK_SIZE as i32;
    let base_z = pos.y * CHUNK_SIZE as i32;
    let mut mask = vec![0u8; CHUNK_SIZE * CHUNK_SIZE];

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let wx = base_x + x as i32;
            let wz = base_z + z as i32;
            let water_cell_solid = is_solid_block(chunks, wx, SEA_LEVEL, wz);
            if water_cell_solid {
                continue;
            }

            let surface = find_surface_y(chunks, wx, wz);
            if surface < SEA_LEVEL {
                mask[x + z * CHUNK_SIZE] = 1;
            }
        }
    }

    mask
}

fn find_surface_y(chunks: &HashMap<IVec2, Chunk>, x: i32, z: i32) -> i32 {
    for y in (0..=SEA_LEVEL + 24).rev() {
        if is_solid_block(chunks, x, y, z) {
            return y;
        }
    }
    0
}

fn is_solid_block(chunks: &HashMap<IVec2, Chunk>, x: i32, y: i32, z: i32) -> bool {
    let b = get_block_world(chunks, x, y, z);
    b != Block::Air && b != Block::Leaves
}

fn water_height(level: u8) -> f32 {
    let l = (level as f32 / 8.0).clamp(0.0, 1.0);
    0.90 + l * 0.10
}

fn ensure_source_mask_for_chunk(sim: &mut WaterFlowSim, chunks: &HashMap<IVec2, Chunk>, pos: IVec2) {
    if sim.source_masks.contains_key(&pos) {
        return;
    }
    let mask = rebuild_source_mask_for_chunk(pos, chunks);
    sim.source_masks.insert(pos, mask);
}

fn has_nearby_water(cell: IVec3, chunks: &HashMap<IVec2, Chunk>, sim: &WaterFlowSim) -> bool {
    for dy in -1..=1 {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let c = cell + IVec3::new(dx, dy, dz);
                if sim.levels.get(&c).copied().unwrap_or(0) > 0 || is_source_cell(c, chunks, sim) {
                    return true;
                }
            }
        }
    }
    false
}

fn has_nearby_dynamic_water(cell: IVec3, sim: &WaterFlowSim) -> bool {
    for dy in -1..=1 {
        for dz in -1..=1 {
            for dx in -1..=1 {
                let c = cell + IVec3::new(dx, dy, dz);
                if sim.levels.get(&c).copied().unwrap_or(0) > 0 {
                    return true;
                }
            }
        }
    }
    false
}

pub fn maintain_water_source_masks(world: Res<VoxelWorld>, mut sim: ResMut<WaterFlowSim>) {
    for pos in world.chunks.keys().copied() {
        if sim.source_masks.contains_key(&pos) {
            continue;
        }
        let mask = rebuild_source_mask_for_chunk(pos, &world.chunks);
        sim.source_masks.insert(pos, mask);
        sim.mark_chunk_dirty(pos);
    }
}

pub fn clear_water_sim_on_world_reset_keys(
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<PromptInputState>,
    mut sim: ResMut<WaterFlowSim>,
) {
    if prompt.active {
        return;
    }
    if keys.just_pressed(KeyCode::KeyR) || keys.just_pressed(KeyCode::F5) || keys.just_pressed(KeyCode::F6) {
        sim.clear();
    }
}

pub fn toggle_water_physics_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    prompt: Res<PromptInputState>,
    mut cfg: ResMut<WaterPhysicsConfig>,
    mut sim: ResMut<WaterFlowSim>,
) {
    if prompt.active || !keys.just_pressed(KeyCode::F9) {
        return;
    }
    cfg.enabled = !cfg.enabled;
    if !cfg.enabled {
        sim.clear();
    }
    info!(
        "water physics {}",
        if cfg.enabled { "enabled" } else { "disabled" }
    );
}
