//! Block targeting, break/place input handling, and placement palette state.
use bevy::prelude::*;
use bevy::input::mouse::MouseWheel;
use bevy::input::{ButtonState, mouse::MouseButtonInput};

use crate::config::{BREAK_REACH, CHUNK_SIZE};
use crate::generation::PromptInputState;
use crate::net_client::NetClientState;
use crate::player::{collides_player, FlyCam};
use crate::world::{
    div_floor, get_block_world, remesh_chunk, set_block_world, Block, LoadedChunks, VoxelWorld,
};

#[derive(Event, Clone, Copy, Debug)]
pub struct LocalBlockEditEvent {
    pub x: i32,
    pub y: i32,
    pub z: i32,
    pub block: Block,
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
    cam_q: Query<&Transform, With<FlyCam>>,
    mut world: ResMut<VoxelWorld>,
    loaded: Res<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut edits: EventWriter<LocalBlockEditEvent>,
    prompt: Res<PromptInputState>,
) {
    if let Some(net) = net
        && net.cfg.enabled
        && net.connected
    {
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

    let Some(hit) = raycast_blocks(cam.translation, *cam.forward(), &world.chunks, BREAK_REACH) else {
        return;
    };

    if set_block_world(&mut world.chunks, hit.solid.x, hit.solid.y, hit.solid.z, Block::Air) {
        edits.send(LocalBlockEditEvent {
            x: hit.solid.x,
            y: hit.solid.y,
            z: hit.solid.z,
            block: Block::Air,
        });
        remesh_at_cell_immediate(hit.solid, &world.chunks, &loaded, &mut meshes);
    }
}

pub fn place_targeted_block(
    mut mouse_events: EventReader<MouseButtonInput>,
    net: Option<Res<NetClientState>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut world: ResMut<VoxelWorld>,
    loaded: Res<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<PlacementPalette>,
    mut edits: EventWriter<LocalBlockEditEvent>,
    prompt: Res<PromptInputState>,
) {
    if let Some(net) = net
        && net.cfg.enabled
        && net.connected
    {
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

    let Some(hit) = raycast_blocks(cam.translation, *cam.forward(), &world.chunks, BREAK_REACH) else {
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

    if !set_block_world(
        &mut world.chunks,
        hit.previous_air.x,
        hit.previous_air.y,
        hit.previous_air.z,
        palette.selected_block(),
    ) {
        return;
    }

    if collides_player(cam.translation, &world.chunks) {
        let _ = set_block_world(
            &mut world.chunks,
            hit.previous_air.x,
            hit.previous_air.y,
            hit.previous_air.z,
            Block::Air,
        );
        return;
    }

    remesh_at_cell_immediate(hit.previous_air, &world.chunks, &loaded, &mut meshes);
    edits.send(LocalBlockEditEvent {
        x: hit.previous_air.x,
        y: hit.previous_air.y,
        z: hit.previous_air.z,
        block: palette.selected_block(),
    });
}

pub fn highlight_targeted_block(
    mut gizmos: Gizmos,
    cam_q: Query<&Transform, With<FlyCam>>,
    world: Res<VoxelWorld>,
) {
    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let Some(hit) = raycast_blocks(cam.translation, *cam.forward(), &world.chunks, BREAK_REACH) else {
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

fn remesh_at_cell_immediate(
    cell: IVec3,
    chunks: &std::collections::HashMap<IVec2, crate::world::Chunk>,
    loaded: &LoadedChunks,
    meshes: &mut Assets<Mesh>,
) {
    let chunk = IVec2::new(
        div_floor(cell.x, CHUNK_SIZE as i32),
        div_floor(cell.z, CHUNK_SIZE as i32),
    );
    remesh_chunk(chunk, chunks, loaded, meshes);
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
