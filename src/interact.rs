//! Player block interaction systems: targeting, break/place, palette/inventory.
//! Owns raycast-based edit actions, local inventory accounting, and placement
//! constraints (collision/authority) so building logic stays centralized.
use bevy::input::mouse::MouseWheel;
use bevy::input::{mouse::MouseButtonInput, ButtonState};
use bevy::prelude::*;

use crate::block_edit::{self, BlockMutationRequest};
use crate::config::{
    BREAK_REACH, CHUNK_SIZE, EYE_HEIGHT, PLAYER_HEIGHT, PLAYER_RADIUS, WORLD_HEIGHT,
};
use crate::generation::PromptInputState;
use crate::net_client::{is_remote_simulation, NetClientState};
use crate::physics::CollisionAabb;
use crate::player::FlyCam;
use crate::world::{div_floor, get_block_world, Block, VoxelWorld};

#[derive(Resource)]
pub struct BlockInventory {
    counts: std::collections::HashMap<Block, u32>,
}

impl Default for BlockInventory {
    fn default() -> Self {
        use Block::*;
        let mut counts = std::collections::HashMap::new();
        for b in [
            Stone, Dirt, Grass, Sand, Wood, Leaves, Red, Blue, Yellow, Purple, Cyan,
        ] {
            counts.insert(b, 0);
        }
        Self { counts }
    }
}

impl BlockInventory {
    pub fn count(&self, block: Block) -> u32 {
        self.counts.get(&block).copied().unwrap_or(0)
    }

    pub fn add(&mut self, block: Block, amount: u32) {
        if block == Block::Air {
            return;
        }
        let e = self.counts.entry(block).or_insert(0);
        *e = e.saturating_add(amount);
    }

    pub fn try_take(&mut self, block: Block, amount: u32) -> bool {
        if block == Block::Air {
            return false;
        }
        let current = self.count(block);
        if current < amount {
            return false;
        }
        self.counts.insert(block, current - amount);
        true
    }
}

#[derive(Clone, Copy)]
struct BlockHit {
    solid: IVec3,
    previous_air: IVec3,
}

#[derive(Resource)]
pub struct PlacementPalette {
    blocks: Vec<(Block, &'static str)>,
    index: usize,
}

impl Default for PlacementPalette {
    fn default() -> Self {
        Self {
            blocks: vec![
                (Block::Stone, "Stone"),
                (Block::Dirt, "Dirt"),
                (Block::Grass, "Grass"),
                (Block::Sand, "Sand"),
                (Block::Wood, "Wood"),
                (Block::Leaves, "Leaves"),
                (Block::Red, "Red"),
                (Block::Blue, "Blue"),
                (Block::Yellow, "Yellow"),
                (Block::Purple, "Purple"),
                (Block::Cyan, "Cyan"),
            ],
            index: 0,
        }
    }
}

impl PlacementPalette {
    pub fn selected_name(&self) -> &'static str {
        self.blocks[self.index].1
    }
    pub fn selected_block(&self) -> Block {
        self.blocks[self.index].0
    }
    pub fn selected_index(&self) -> usize {
        self.index
    }
    pub fn len(&self) -> usize {
        self.blocks.len()
    }
    pub fn entry(&self, index: usize) -> Option<(Block, &'static str)> {
        self.blocks.get(index).copied()
    }
}

pub fn cycle_palette_on_scroll(
    mut scroll: EventReader<MouseWheel>,
    mut palette: ResMut<PlacementPalette>,
    prompt: Res<PromptInputState>,
) {
    if prompt.active {
        scroll.clear();
        return;
    }

    let mut delta = 0.0f32;
    for e in scroll.read() {
        delta += e.y;
    }
    if delta == 0.0 {
        return;
    }

    if delta > 0.0 {
        palette.index = (palette.index + 1) % palette.blocks.len();
    } else if palette.index == 0 {
        palette.index = palette.blocks.len() - 1;
    } else {
        palette.index -= 1;
    }
}

pub fn break_targeted_block(
    mut mouse_events: EventReader<MouseButtonInput>,
    net: Option<Res<NetClientState>>,
    rider: Option<Res<crate::vehicles::VehicleRiderState>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    world: Res<VoxelWorld>,
    mut inv: ResMut<BlockInventory>,
    mut edits: EventWriter<BlockMutationRequest>,
    prompt: Res<PromptInputState>,
) {
    if is_remote_simulation(net.as_deref()) {
        return;
    }
    if rider.as_deref().map(|r| r.is_mounted()).unwrap_or(false) {
        mouse_events.clear();
        return;
    }
    if prompt.active {
        return;
    }
    let mouse_break = mouse_events
        .read()
        .any(|e| e.button == MouseButton::Left && e.state == ButtonState::Pressed);
    if !mouse_break {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let Some(hit) = raycast_blocks(cam.translation, *cam.forward(), &world.chunks, BREAK_REACH)
    else {
        return;
    };

    let broken = get_block_world(&world.chunks, hit.solid.x, hit.solid.y, hit.solid.z);
    if broken == Block::Air || !can_write_cell(&world.chunks, hit.solid) {
        return;
    }

    inv.add(broken, 1);
    block_edit::enqueue_block_mutation(
        &mut edits,
        hit.solid.x,
        hit.solid.y,
        hit.solid.z,
        Block::Air,
        true,
    );
}

pub fn place_targeted_block(
    mut mouse_events: EventReader<MouseButtonInput>,
    net: Option<Res<NetClientState>>,
    rider: Option<Res<crate::vehicles::VehicleRiderState>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    world: Res<VoxelWorld>,
    palette: Res<PlacementPalette>,
    mut inv: ResMut<BlockInventory>,
    mut edits: EventWriter<BlockMutationRequest>,
    prompt: Res<PromptInputState>,
) {
    if is_remote_simulation(net.as_deref()) {
        return;
    }
    if rider.as_deref().map(|r| r.is_mounted()).unwrap_or(false) {
        mouse_events.clear();
        return;
    }
    if prompt.active {
        return;
    }
    let mouse_place = mouse_events
        .read()
        .any(|e| e.button == MouseButton::Right && e.state == ButtonState::Pressed);
    if !mouse_place {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let Some(hit) = raycast_blocks(cam.translation, *cam.forward(), &world.chunks, BREAK_REACH)
    else {
        return;
    };

    if get_block_world(
        &world.chunks,
        hit.previous_air.x,
        hit.previous_air.y,
        hit.previous_air.z,
    ) != Block::Air
    {
        return;
    }

    let place_block = palette.selected_block();
    if !inv.try_take(place_block, 1) {
        return;
    }

    if !can_write_cell(&world.chunks, hit.previous_air) {
        inv.add(place_block, 1);
        return;
    }

    if player_intersects_cell(cam.translation, hit.previous_air) {
        inv.add(place_block, 1);
        return;
    }

    block_edit::enqueue_block_mutation(
        &mut edits,
        hit.previous_air.x,
        hit.previous_air.y,
        hit.previous_air.z,
        place_block,
        true,
    );
}

pub fn highlight_targeted_block(
    mut gizmos: Gizmos,
    rider: Option<Res<crate::vehicles::VehicleRiderState>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    world: Res<VoxelWorld>,
) {
    if rider.as_deref().map(|r| r.is_mounted()).unwrap_or(false) {
        return;
    }
    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let Some(hit) = raycast_blocks(cam.translation, *cam.forward(), &world.chunks, BREAK_REACH)
    else {
        return;
    };

    let center = Vec3::new(
        hit.solid.x as f32 + 0.5,
        hit.solid.y as f32 + 0.5,
        hit.solid.z as f32 + 0.5,
    );
    let transform = Transform::from_translation(center).with_scale(Vec3::splat(1.01));
    gizmos.cuboid(transform, Color::srgba(0.95, 0.95, 0.95, 0.95));
}

fn raycast_blocks(
    origin: Vec3,
    dir: Vec3,
    chunks: &std::collections::HashMap<IVec2, crate::world::Chunk>,
    max_dist: f32,
) -> Option<BlockHit> {
    let step = 0.05;
    let mut t = 0.0;
    let mut last_cell = IVec3::new(i32::MIN, i32::MIN, i32::MIN);
    let mut last_air = IVec3::new(i32::MIN, i32::MIN, i32::MIN);

    while t <= max_dist {
        let p = origin + dir * t;
        let cell = IVec3::new(p.x.floor() as i32, p.y.floor() as i32, p.z.floor() as i32);
        if cell == last_cell {
            t += step;
            continue;
        }
        last_cell = cell;

        if get_block_world(chunks, cell.x, cell.y, cell.z) == Block::Air {
            last_air = cell;
            t += step;
            continue;
        }

        if last_air.x == i32::MIN {
            return None;
        }

        return Some(BlockHit {
            solid: cell,
            previous_air: last_air,
        });
    }

    None
}

fn can_write_cell(
    chunks: &std::collections::HashMap<IVec2, crate::world::Chunk>,
    cell: IVec3,
) -> bool {
    if !(0..WORLD_HEIGHT as i32).contains(&cell.y) {
        return false;
    }
    let chunk = IVec2::new(
        div_floor(cell.x, CHUNK_SIZE as i32),
        div_floor(cell.z, CHUNK_SIZE as i32),
    );
    chunks.contains_key(&chunk)
}

fn player_intersects_cell(eye_pos: Vec3, cell: IVec3) -> bool {
    let body = CollisionAabb::from_eye(eye_pos, PLAYER_RADIUS, EYE_HEIGHT, PLAYER_HEIGHT);
    let cell_min = Vec3::new(cell.x as f32, cell.y as f32, cell.z as f32);
    let cell_max = cell_min + Vec3::ONE;
    body.max.x > cell_min.x
        && body.min.x < cell_max.x
        && body.max.y > cell_min.y
        && body.min.y < cell_max.y
        && body.max.z > cell_min.z
        && body.min.z < cell_max.z
}
