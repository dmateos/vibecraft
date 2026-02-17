use bevy::prelude::*;

use crate::config::{BREAK_REACH, CHUNK_SIZE};
use crate::player::{collides_player, FlyCam};
use crate::world::{
    div_floor, get_block_world, remesh_affected_chunks, set_block_world, Block, LoadedChunks, VoxelWorld,
};

#[derive(Clone, Copy)]
struct BlockHit {
    solid: IVec3,
    previous_air: IVec3,
}

pub fn break_targeted_block(
    buttons: Res<ButtonInput<MouseButton>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut world: ResMut<VoxelWorld>,
    loaded: Res<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !buttons.just_pressed(MouseButton::Left) {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };

    let Some(hit) = raycast_blocks(cam.translation, *cam.forward(), &world.chunks, BREAK_REACH) else {
        return;
    };

    if set_block_world(&mut world.chunks, hit.solid.x, hit.solid.y, hit.solid.z, Block::Air) {
        remesh_at_cell(hit.solid, &world.chunks, &loaded, &mut meshes);
    }
}

pub fn place_targeted_block(
    buttons: Res<ButtonInput<MouseButton>>,
    cam_q: Query<&Transform, With<FlyCam>>,
    mut world: ResMut<VoxelWorld>,
    loaded: Res<LoadedChunks>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    if !buttons.just_pressed(MouseButton::Right) {
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
        Block::Stone,
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

    remesh_at_cell(hit.previous_air, &world.chunks, &loaded, &mut meshes);
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

fn remesh_at_cell(
    cell: IVec3,
    chunks: &std::collections::HashMap<IVec2, crate::world::Chunk>,
    loaded: &LoadedChunks,
    meshes: &mut Assets<Mesh>,
) {
    let chunk = IVec2::new(
        div_floor(cell.x, CHUNK_SIZE as i32),
        div_floor(cell.z, CHUNK_SIZE as i32),
    );
    remesh_affected_chunks(chunk, chunks, loaded, meshes);
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
