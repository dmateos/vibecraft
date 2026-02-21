use bevy::prelude::*;

use crate::perception;
use crate::physics;
use crate::player::FlyCam;
use crate::ui::DebugOverlayState;
use crate::world::VoxelWorld;

use super::{HOSTILE_VISION_DOT, HOSTILE_VISION_RANGE, NPC_HEIGHT};
use super::{Npc, NpcKind};

pub fn draw_npc_debug_gizmos(
    overlay: Res<DebugOverlayState>,
    world: Res<VoxelWorld>,
    cam_q: Query<&Transform, With<FlyCam>>,
    npc_q: Query<(&Transform, &Npc)>,
    mut gizmos: Gizmos,
) {
    if !overlay.visible {
        return;
    }

    let Ok(cam) = cam_q.get_single() else {
        return;
    };
    let cam_pos = cam.translation;

    let mut nearest: Option<(f32, Vec3, &Npc)> = None;
    for (t, npc) in &npc_q {
        let d = cam_pos.distance(t.translation);
        if nearest.map(|n| d < n.0).unwrap_or(true) {
            nearest = Some((d, t.translation, npc));
        }
    }
    let Some((_dist, npc_pos, npc)) = nearest else {
        return;
    };

    let body_center = npc_pos + Vec3::new(0.0, NPC_HEIGHT * 0.5, 0.0);
    let eye = npc_pos + Vec3::new(0.0, 1.45, 0.0);
    let heading = Vec3::new(npc.heading.cos(), 0.0, npc.heading.sin()).normalize_or_zero();
    let heading_color = if npc.stuck_timer > 0.2 {
        Color::srgb(1.0, 0.55, 0.18)
    } else {
        Color::srgb(0.16, 0.92, 0.36)
    };

    gizmos.cuboid(
        Transform::from_translation(body_center).with_scale(Vec3::new(0.72, NPC_HEIGHT, 0.72)),
        Color::srgba(1.0, 1.0, 1.0, 0.38),
    );
    gizmos.line(eye, eye + heading * 3.2, heading_color);

    let target = match npc.kind {
        NpcKind::Friendly => {
            if npc.follow_player {
                Some(cam_pos)
            } else {
                None
            }
        }
        NpcKind::Hostile => {
            if npc.last_seen_timer > 0.05 {
                Some(npc.last_seen_player)
            } else if npc.investigate_timer > 0.05 {
                Some(npc.investigate_target)
            } else {
                None
            }
        }
    };

    if let Some(target_pos) = target {
        let marker = target_pos + Vec3::new(0.0, 0.6, 0.0);
        gizmos.line(eye, marker, Color::srgb(0.24, 0.62, 1.0));
        gizmos.cuboid(
            Transform::from_translation(marker).with_scale(Vec3::splat(0.35)),
            Color::srgba(0.24, 0.62, 1.0, 0.85),
        );
    }

    if npc.kind == NpcKind::Hostile {
        let sees = perception::can_see_target(
            npc_pos,
            npc.heading,
            1.45,
            cam_pos,
            1.45,
            HOSTILE_VISION_RANGE,
            HOSTILE_VISION_DOT,
            &world.chunks,
            physics::is_solid_for_npc,
        );
        let los_color = if sees {
            Color::srgb(0.95, 0.10, 0.10)
        } else if perception::line_of_sight_clear(
            eye,
            cam_pos + Vec3::new(0.0, 1.3, 0.0),
            &world.chunks,
            physics::is_solid_for_npc,
        ) {
            Color::srgb(0.94, 0.84, 0.18)
        } else {
            Color::srgb(0.45, 0.45, 0.45)
        };
        gizmos.line(eye, cam_pos + Vec3::new(0.0, 1.3, 0.0), los_color);
    }
}
