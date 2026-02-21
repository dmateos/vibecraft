use bevy::prelude::*;
use noise::NoiseFn;

use crate::config::{CHUNK_SIZE, SEA_LEVEL, WORLD_HEIGHT};

use super::generation::{TerrainNoise, sample_surface};
use super::{BiomeKind, Block, Chunk, div_floor};
enum TreeKind {
    Oak,
    Pine,
}

pub(super) fn stamp_trees(chunk: &mut Chunk, noise: &TerrainNoise) {
    const TREE_CELL: i32 = 9;
    const TREE_MARGIN: i32 = 6;

    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let min_x = base_x - TREE_MARGIN;
    let max_x = base_x + CHUNK_SIZE as i32 - 1 + TREE_MARGIN;
    let min_z = base_z - TREE_MARGIN;
    let max_z = base_z + CHUNK_SIZE as i32 - 1 + TREE_MARGIN;

    let cell_min_x = div_floor(min_x, TREE_CELL);
    let cell_max_x = div_floor(max_x, TREE_CELL);
    let cell_min_z = div_floor(min_z, TREE_CELL);
    let cell_max_z = div_floor(max_z, TREE_CELL);

    for cz in cell_min_z..=cell_max_z {
        for cx in cell_min_x..=cell_max_x {
            let h = hash3(cx, cz, noise.seed ^ 0x2A6B_45D1);
            let tx = cx * TREE_CELL + (h as i32 & 7);
            let tz = cz * TREE_CELL + (((h >> 8) as i32) & 7);

            if tx < min_x || tx > max_x || tz < min_z || tz > max_z {
                continue;
            }

            let sample = sample_surface(noise, tx, tz);
            let sy = sample.height as i32;
            if sy <= SEA_LEVEL + 2 {
                continue;
            }
            if sample.ridge.abs() > 0.68 {
                continue;
            }
            let roll = ((h >> 16) & 0xFF) as i32;
            let (threshold, kind) = match sample.biome {
                BiomeKind::Desert => (255, TreeKind::Oak),
                BiomeKind::Rocky => (250, TreeKind::Pine),
                BiomeKind::Tundra => (246, TreeKind::Pine),
                BiomeKind::Plains => (214, TreeKind::Oak),
                BiomeKind::Swamp => (190, TreeKind::Oak),
                BiomeKind::Forest => (
                    158,
                    if (h & 1) == 0 {
                        TreeKind::Oak
                    } else {
                        TreeKind::Pine
                    },
                ),
            };
            if roll < threshold {
                continue;
            }

            let local_seed = hash3(tx, tz, noise.seed ^ 0x91C2_8E4B);
            place_tree(chunk, tx, sy + 1, tz, kind, local_seed);
        }
    }
}

pub(super) fn stamp_dense_forest(chunk: &mut Chunk, noise: &TerrainNoise) {
    const GROVE_CELL: i32 = 14;
    const GROVE_MARGIN: i32 = 10;

    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let min_x = base_x - GROVE_MARGIN;
    let max_x = base_x + CHUNK_SIZE as i32 - 1 + GROVE_MARGIN;
    let min_z = base_z - GROVE_MARGIN;
    let max_z = base_z + CHUNK_SIZE as i32 - 1 + GROVE_MARGIN;

    let cell_min_x = div_floor(min_x, GROVE_CELL);
    let cell_max_x = div_floor(max_x, GROVE_CELL);
    let cell_min_z = div_floor(min_z, GROVE_CELL);
    let cell_max_z = div_floor(max_z, GROVE_CELL);

    for cz in cell_min_z..=cell_max_z {
        for cx in cell_min_x..=cell_max_x {
            let h = hash3(cx, cz, noise.seed ^ 0x7D31_A4E2);
            let roll = (h & 0xFF) as i32;
            if roll < 156 {
                continue;
            }

            let gx = cx * GROVE_CELL + (((h >> 8) as i32 & 13) - 6);
            let gz = cz * GROVE_CELL + (((h >> 16) as i32 & 13) - 6);
            if gx < min_x || gx > max_x || gz < min_z || gz > max_z {
                continue;
            }

            let center = sample_surface(noise, gx, gz);
            if center.biome != BiomeKind::Forest {
                continue;
            }
            if center.height as i32 <= SEA_LEVEL + 3 || center.ridge.abs() > 0.55 {
                continue;
            }
            let moisture = noise
                .perlin_moisture
                .get([gx as f64 * 0.0022 + 121.3, gz as f64 * 0.0022 - 87.1]) as f32;
            if moisture < 0.30 {
                continue;
            }

            let trees = 3 + ((h >> 24) as i32 & 0x3);
            for i in 0..trees {
                let th = hash3(gx + i * 7, gz - i * 11, noise.seed ^ 0xA17C_22D1);
                let ox = (((th >> 4) as i32 & 7) - 3).clamp(-3, 3);
                let oz = (((th >> 12) as i32 & 7) - 3).clamp(-3, 3);
                let tx = gx + ox;
                let tz = gz + oz;
                if tx < min_x || tx > max_x || tz < min_z || tz > max_z {
                    continue;
                }

                let s = sample_surface(noise, tx, tz);
                let sy = s.height as i32;
                if s.biome != BiomeKind::Forest || sy <= SEA_LEVEL + 2 || s.ridge.abs() > 0.60 {
                    continue;
                }

                let kind = if (th & 3) == 0 { TreeKind::Pine } else { TreeKind::Oak };
                let local_seed = hash3(tx, tz, noise.seed ^ 0x54DA_88E9);
                place_tree(chunk, tx, sy + 1, tz, kind, local_seed);
            }
        }
    }
}

pub(super) fn stamp_biome_features(chunk: &mut Chunk, noise: &TerrainNoise) {
    const FEATURE_CELL: i32 = 10;
    const FEATURE_MARGIN: i32 = 7;

    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let min_x = base_x - FEATURE_MARGIN;
    let max_x = base_x + CHUNK_SIZE as i32 - 1 + FEATURE_MARGIN;
    let min_z = base_z - FEATURE_MARGIN;
    let max_z = base_z + CHUNK_SIZE as i32 - 1 + FEATURE_MARGIN;

    let cell_min_x = div_floor(min_x, FEATURE_CELL);
    let cell_max_x = div_floor(max_x, FEATURE_CELL);
    let cell_min_z = div_floor(min_z, FEATURE_CELL);
    let cell_max_z = div_floor(max_z, FEATURE_CELL);

    for cz in cell_min_z..=cell_max_z {
        for cx in cell_min_x..=cell_max_x {
            let h = hash3(cx, cz, noise.seed ^ 0x4D2A_9C17);
            let tx = cx * FEATURE_CELL + ((h as i32) & 7);
            let tz = cz * FEATURE_CELL + (((h >> 8) as i32) & 7);
            if tx < min_x || tx > max_x || tz < min_z || tz > max_z {
                continue;
            }

            let sample = sample_surface(noise, tx, tz);
            let sy = sample.height as i32;
            if sy <= SEA_LEVEL + 1 {
                continue;
            }
            if sample.ridge.abs() > 0.76 {
                continue;
            }

            let roll = ((h >> 16) & 0xFF) as i32;
            match sample.biome {
                BiomeKind::Rocky => {
                    if roll >= 216 {
                        let radius = 1 + ((h >> 3) % 2) as i32;
                        place_boulder(chunk, tx, sy + 1, tz, radius, Block::Stone, h);
                    }
                }
                BiomeKind::Desert => {
                    if roll >= 224 {
                        let radius = 1 + ((h >> 5) % 2) as i32;
                        place_boulder(chunk, tx, sy + 1, tz, radius, Block::Sand, h);
                    }
                }
                BiomeKind::Swamp => {
                    if sy <= SEA_LEVEL + 6 && roll >= 200 {
                        place_reed_clump(chunk, tx, sy + 1, tz, h);
                    }
                }
                BiomeKind::Forest | BiomeKind::Plains => {}
                BiomeKind::Tundra => {
                    if roll >= 242 {
                        place_boulder(chunk, tx, sy + 1, tz, 1, Block::Stone, h);
                    }
                }
            }
        }
    }
}

pub(super) fn stamp_villages(chunk: &mut Chunk, noise: &TerrainNoise) {
    const VILLAGE_CELL: i32 = 80;
    const VILLAGE_MARGIN: i32 = 56;

    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let min_x = base_x - VILLAGE_MARGIN;
    let max_x = base_x + CHUNK_SIZE as i32 - 1 + VILLAGE_MARGIN;
    let min_z = base_z - VILLAGE_MARGIN;
    let max_z = base_z + CHUNK_SIZE as i32 - 1 + VILLAGE_MARGIN;

    let cell_min_x = div_floor(min_x, VILLAGE_CELL);
    let cell_max_x = div_floor(max_x, VILLAGE_CELL);
    let cell_min_z = div_floor(min_z, VILLAGE_CELL);
    let cell_max_z = div_floor(max_z, VILLAGE_CELL);

    for cz in cell_min_z..=cell_max_z {
        for cx in cell_min_x..=cell_max_x {
            let h = hash3(cx, cz, noise.seed ^ 0x51AA_92F1);
            let guaranteed_origin = cx == 0 && cz == 0;
            if !guaranteed_origin && (h & 0xFF) < 232 {
                continue;
            }

            let vx = cx * VILLAGE_CELL + (((((h >> 8) as i32).rem_euclid(VILLAGE_CELL)) - (VILLAGE_CELL / 2)));
            let vz = cz * VILLAGE_CELL + (((((h >> 16) as i32).rem_euclid(VILLAGE_CELL)) - (VILLAGE_CELL / 2)));
            if vx < min_x || vx > max_x || vz < min_z || vz > max_z {
                continue;
            }

            let center = sample_surface(noise, vx, vz);
            if !guaranteed_origin
                && !matches!(center.biome, BiomeKind::Plains | BiomeKind::Forest | BiomeKind::Swamp)
            {
                continue;
            }
            if !guaranteed_origin
                && (center.height as i32 <= SEA_LEVEL + 3 || center.height as i32 >= SEA_LEVEL + 34)
            {
                continue;
            }

            let r = 18;
            let s1 = sample_surface(noise, vx - r, vz).height as i32;
            let s2 = sample_surface(noise, vx + r, vz).height as i32;
            let s3 = sample_surface(noise, vx, vz - r).height as i32;
            let s4 = sample_surface(noise, vx, vz + r).height as i32;
            let max_h = s1.max(s2).max(s3).max(s4).max(center.height as i32);
            let min_h = s1.min(s2).min(s3).min(s4).min(center.height as i32);
            if !guaranteed_origin && max_h - min_h > 10 {
                continue;
            }

            place_village(chunk, vx, center.height as i32 + 1, vz, h);
        }
    }
}

fn place_village(chunk: &mut Chunk, cx: i32, ground_y: i32, cz: i32, seed: u32) {
    let radius = 30 + ((seed >> 27) & 7) as i32;
    flatten_village_ground(chunk, cx, ground_y, cz, radius + 6);

    // Main roads.
    for dz in -2..=2 {
        for dx in -radius..=radius {
            set_if_inside(chunk, cx + dx, ground_y, cz + dz, Block::Dirt);
        }
    }
    for dx in -2..=2 {
        for dz in -radius..=radius {
            set_if_inside(chunk, cx + dx, ground_y, cz + dz, Block::Dirt);
        }
    }

    // Ring road.
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            let d2 = dx * dx + dz * dz;
            if d2 >= (radius - 2) * (radius - 2) && d2 <= radius * radius {
                set_if_inside(chunk, cx + dx, ground_y, cz + dz, Block::Stone);
            }
        }
    }

    // Central square.
    for dz in -6_i32..=6_i32 {
        for dx in -6_i32..=6_i32 {
            let paver = if (dx + dz).abs() % 3 == 0 { Block::Stone } else { Block::Dirt };
            set_if_inside(chunk, cx + dx, ground_y, cz + dz, paver);
        }
    }

    // Market marker / fountain core.
    for y in ground_y + 1..=ground_y + 3 {
        set_if_inside(chunk, cx, y, cz, Block::Yellow);
    }

    let house_spots = [
        IVec2::new(-15, -10),
        IVec2::new(-7, -14),
        IVec2::new(8, -14),
        IVec2::new(15, -9),
        IVec2::new(-15, 9),
        IVec2::new(-8, 14),
        IVec2::new(8, 14),
        IVec2::new(14, 10),
        IVec2::new(-2, -18),
        IVec2::new(2, 18),
        IVec2::new(-24, -8),
        IVec2::new(-24, 8),
        IVec2::new(24, -8),
        IVec2::new(24, 8),
        IVec2::new(-10, -24),
        IVec2::new(10, -24),
        IVec2::new(-10, 24),
        IVec2::new(10, 24),
        IVec2::new(-20, -18),
        IVec2::new(20, 18),
    ];
    let count = 10 + ((seed >> 20) & 0x7) as usize;
    for i in 0..count.min(house_spots.len()) {
        let idx = ((i as u32 * 5 + (seed >> 5)) as usize) % house_spots.len();
        let spot = house_spots[idx];
        let hw = 4 + ((seed >> (i + 2)) & 1) as i32;
        let hd = 4 + ((seed >> (i + 8)) & 1) as i32;
        let hh = 4 + ((seed >> (i + 12)) & 2) as i32;
        place_house(chunk, cx + spot.x, ground_y + 1, cz + spot.y, hw, hd, hh.clamp(4, 7), seed ^ i as u32);
    }
}

pub(super) fn stamp_grand_monument(chunk: &mut Chunk, noise: &TerrainNoise) {
    let center = monument_center(noise.seed);
    let cx = center.x;
    let cz = center.y;
    let half_w = 36;
    let half_d = 30;
    let influence = 58;

    let chunk_min_x = chunk.pos.x * CHUNK_SIZE as i32;
    let chunk_max_x = chunk_min_x + CHUNK_SIZE as i32 - 1;
    let chunk_min_z = chunk.pos.y * CHUNK_SIZE as i32;
    let chunk_max_z = chunk_min_z + CHUNK_SIZE as i32 - 1;
    if cx + influence < chunk_min_x
        || cx - influence > chunk_max_x
        || cz + influence < chunk_min_z
        || cz - influence > chunk_max_z
    {
        return;
    }

    let base_y = (sample_surface(noise, cx, cz).height as i32 + 1).clamp(SEA_LEVEL + 7, WORLD_HEIGHT as i32 - 48);
    flatten_village_ground(chunk, cx, base_y, cz, influence);

    // Raised stone plinth.
    for z in -half_d - 3..=half_d + 3 {
        for x in -half_w - 3..=half_w + 3 {
            let wx = cx + x;
            let wz = cz + z;
            if x * x + z * z > (influence + 6) * (influence + 6) {
                continue;
            }
            set_if_inside(chunk, wx, base_y - 1, wz, Block::Stone);
            set_if_inside(chunk, wx, base_y, wz, Block::Stone);
        }
    }

    // Courtyard fill.
    for z in -half_d + 2..=half_d - 2 {
        for x in -half_w + 2..=half_w - 2 {
            set_if_inside(chunk, cx + x, base_y + 1, cz + z, Block::Stone);
        }
    }

    // Massive outer wall.
    let wall_h = 17;
    for y in base_y + 2..=base_y + wall_h {
        for x in -half_w..=half_w {
            for t in 0..=1 {
                set_if_inside(chunk, cx + x, y, cz - half_d + t, Block::Stone);
                set_if_inside(chunk, cx + x, y, cz + half_d - t, Block::Stone);
            }
        }
        for z in -half_d..=half_d {
            for t in 0..=1 {
                set_if_inside(chunk, cx - half_w + t, y, cz + z, Block::Stone);
                set_if_inside(chunk, cx + half_w - t, y, cz + z, Block::Stone);
            }
        }
    }

    // Battlements.
    for x in (-half_w..=half_w).step_by(2) {
        set_if_inside(chunk, cx + x, base_y + wall_h + 1, cz - half_d, Block::Snow);
        set_if_inside(chunk, cx + x, base_y + wall_h + 1, cz + half_d, Block::Snow);
    }
    for z in (-half_d..=half_d).step_by(2) {
        set_if_inside(chunk, cx - half_w, base_y + wall_h + 1, cz + z, Block::Snow);
        set_if_inside(chunk, cx + half_w, base_y + wall_h + 1, cz + z, Block::Snow);
    }

    // Gate and bridge.
    for y in base_y + 2..=base_y + 9 {
        for x in -4..=4 {
            set_if_inside(chunk, cx + x, y, cz - half_d, Block::Air);
            set_if_inside(chunk, cx + x, y, cz - half_d + 1, Block::Air);
        }
    }
    for z in -half_d - 12..=-half_d + 3 {
        for x in -5..=5 {
            set_if_inside(chunk, cx + x, base_y + 1, cz + z, Block::Stone);
        }
    }

    // Corner towers.
    for (tx, tz) in [
        (cx - half_w + 2, cz - half_d + 2),
        (cx + half_w - 2, cz - half_d + 2),
        (cx - half_w + 2, cz + half_d - 2),
        (cx + half_w - 2, cz + half_d - 2),
    ] {
        place_round_tower(chunk, tx, base_y + 1, tz, 5, 28);
    }

    // Cathedral body.
    let nave_hw = 12;
    let nave_hd = 21;
    let nave_h = 21;
    for y in base_y + 2..=base_y + nave_h {
        for z in -nave_hd..=nave_hd {
            for x in -nave_hw..=nave_hw {
                let wx = cx + x;
                let wz = cz + z;
                let border = x.abs() >= nave_hw - 1 || z.abs() >= nave_hd - 1;
                if border {
                    let mat = if y % 4 == 0 { Block::Snow } else { Block::Stone };
                    set_if_inside(chunk, wx, y, wz, mat);
                } else {
                    set_if_inside(chunk, wx, y, wz, Block::Air);
                }
            }
        }
    }

    // Aisles.
    for side in [-1, 1] {
        let ax0 = side * (nave_hw + 1);
        let ax1 = side * (nave_hw + 7);
        for y in base_y + 2..=base_y + 12 {
            for z in -nave_hd + 2..=nave_hd - 2 {
                for x in ax0.min(ax1)..=ax0.max(ax1) {
                    let border = x == ax0 || x == ax1 || z.abs() == nave_hd - 2;
                    let wx = cx + x;
                    let wz = cz + z;
                    if border {
                        set_if_inside(chunk, wx, y, wz, Block::Stone);
                    } else {
                        set_if_inside(chunk, wx, y, wz, Block::Air);
                    }
                }
            }
        }
    }

    // Clerestory roofline.
    for ry in 0..=7 {
        let shrink = ry / 2;
        for z in -(nave_hd + 1 - shrink)..=(nave_hd + 1 - shrink) {
            for x in -(nave_hw + 1 - shrink)..=(nave_hw + 1 - shrink) {
                set_if_inside(chunk, cx + x, base_y + nave_h + 1 + ry, cz + z, Block::Wood);
            }
        }
    }

    // Central spire.
    place_round_tower(chunk, cx, base_y + 2, cz + 3, 4, 38);
    for y in base_y + 40..=base_y + 45 {
        let r = (base_y + 45 - y).max(1);
        for z in -r..=r {
            for x in -r..=r {
                if x * x + z * z <= r * r {
                    set_if_inside(chunk, cx + x, y, cz + 3 + z, Block::Snow);
                }
            }
        }
    }

    // Stained window accents.
    for y in (base_y + 6..=base_y + 18).step_by(4) {
        for x in [-nave_hw + 1, nave_hw - 1] {
            set_if_inside(chunk, cx + x, y, cz - 12, Block::Blue);
            set_if_inside(chunk, cx + x, y, cz, Block::Purple);
            set_if_inside(chunk, cx + x, y, cz + 12, Block::Cyan);
        }
        set_if_inside(chunk, cx, y, cz - nave_hd + 1, Block::Red);
    }

    // Keep marker on top so it's easy to spot from distance.
    for y in base_y + 46..=base_y + 49 {
        set_if_inside(chunk, cx, y, cz + 3, Block::Yellow);
    }
}

pub(super) fn stamp_megacity(chunk: &mut Chunk, noise: &TerrainNoise) {
    let center = city_center(noise.seed);
    let cx = center.x;
    let cz = center.y;
    let influence = 188;

    let chunk_min_x = chunk.pos.x * CHUNK_SIZE as i32;
    let chunk_max_x = chunk_min_x + CHUNK_SIZE as i32 - 1;
    let chunk_min_z = chunk.pos.y * CHUNK_SIZE as i32;
    let chunk_max_z = chunk_min_z + CHUNK_SIZE as i32 - 1;
    if cx + influence < chunk_min_x
        || cx - influence > chunk_max_x
        || cz + influence < chunk_min_z
        || cz - influence > chunk_max_z
    {
        return;
    }

    let base_y = (sample_surface(noise, cx, cz).height as i32 + 1).clamp(SEA_LEVEL + 7, WORLD_HEIGHT as i32 - 70);
    flatten_village_ground(chunk, cx, base_y, cz, influence);

    // Hierarchical roads: big avenues + smaller cross streets.
    for z in -152_i32..=152_i32 {
        for x in -152_i32..=152_i32 {
            let ax = x.rem_euclid(24);
            let az = z.rem_euclid(24);
            let sx = x.rem_euclid(12);
            let sz = z.rem_euclid(12);
            let on_avenue = ax <= 2 || ax >= 22 || az <= 2 || az >= 22;
            let on_street = sx == 0 || sz == 0;

            if on_avenue {
                let lane = if (x + z).abs() % 6 == 0 { Block::Stone } else { Block::Dirt };
                set_if_inside(chunk, cx + x, base_y + 1, cz + z, lane);
            } else if on_street {
                set_if_inside(chunk, cx + x, base_y + 1, cz + z, Block::Stone);
            }
        }
    }

    // Ring avenues.
    for z in -166_i32..=166_i32 {
        for x in -166_i32..=166_i32 {
            let d2 = x * x + z * z;
            if (154 * 154..=166 * 166).contains(&d2) {
                set_if_inside(chunk, cx + x, base_y + 1, cz + z, Block::Stone);
            }
            if (122 * 122..=132 * 132).contains(&d2) {
                set_if_inside(chunk, cx + x, base_y + 1, cz + z, Block::Dirt);
            }
        }
    }

    // Building lots.
    for gz in -12_i32..=12_i32 {
        for gx in -12_i32..=12_i32 {
            if gx.abs() <= 1 && gz.abs() <= 1 {
                continue;
            }
            let lot_hash = hash3(gx, gz, noise.seed ^ 0xBB10_6237);
            if lot_hash & 0xF == 0 {
                continue;
            }
            let lot_cx = cx + gx * 12 + (((lot_hash >> 3) & 3) as i32 - 1);
            let lot_cz = cz + gz * 12 + (((lot_hash >> 5) & 3) as i32 - 1);
            if lot_cx < cx - 148 || lot_cx > cx + 148 || lot_cz < cz - 148 || lot_cz > cz + 148 {
                continue;
            }
            let hw = 3 + ((lot_hash >> 8) & 0x2) as i32;
            let hd = 3 + ((lot_hash >> 10) & 0x2) as i32;

            let radial = ((gx * gx + gz * gz) as f32).sqrt();
            let core_boost = (10.0 - radial).max(0.0) * 4.5;
            let h = 14 + ((lot_hash >> 14) & 0x1F) as i32 + core_boost as i32;
            let height = h.clamp(16, 72);

            place_skyscraper(chunk, lot_cx, base_y + 2, lot_cz, hw, hd, height, lot_hash);
        }
    }

    // Signature supertalls.
    for (i, (tx, tz)) in [
        (cx + 16, cz - 12),
        (cx - 18, cz + 14),
        (cx + 3, cz + 24),
        (cx - 5, cz - 26),
    ]
    .into_iter()
    .enumerate()
    {
        place_skyscraper(
            chunk,
            tx,
            base_y + 2,
            tz,
            5 + (i as i32 % 2),
            5 + ((i as i32 + 1) % 2),
            74 - (i as i32 * 5),
            hash3(tx, tz, noise.seed ^ 0xD4A1_9943),
        );
    }

    // Spawn marker for easy spotting from distance.
    for y in base_y + 3..=base_y + 8 {
        set_if_inside(chunk, cx, y, cz, Block::Yellow);
    }
}

pub(super) fn stamp_maze(chunk: &mut Chunk, noise: &TerrainNoise) {
    let center = maze_center(noise.seed);
    let cx = center.x;
    let cz = center.y;
    let influence = 116;

    let chunk_min_x = chunk.pos.x * CHUNK_SIZE as i32;
    let chunk_max_x = chunk_min_x + CHUNK_SIZE as i32 - 1;
    let chunk_min_z = chunk.pos.y * CHUNK_SIZE as i32;
    let chunk_max_z = chunk_min_z + CHUNK_SIZE as i32 - 1;
    if cx + influence < chunk_min_x
        || cx - influence > chunk_max_x
        || cz + influence < chunk_min_z
        || cz - influence > chunk_max_z
    {
        return;
    }

    let base_y = (sample_surface(noise, cx, cz).height as i32 + 1).clamp(SEA_LEVEL + 4, SEA_LEVEL + 18);
    flatten_village_ground(chunk, cx, base_y, cz, influence);

    // Maze slab.
    for z in -96_i32..=96_i32 {
        for x in -96_i32..=96_i32 {
            set_if_inside(chunk, cx + x, base_y + 1, cz + z, Block::Stone);
        }
    }

    // Outer wall ring.
    for y in base_y + 2..=base_y + 8 {
        for z in -96_i32..=96_i32 {
            set_if_inside(chunk, cx - 96, y, cz + z, Block::Stone);
            set_if_inside(chunk, cx + 96, y, cz + z, Block::Stone);
        }
        for x in -96_i32..=96_i32 {
            set_if_inside(chunk, cx + x, y, cz - 96, Block::Stone);
            set_if_inside(chunk, cx + x, y, cz + 96, Block::Stone);
        }
    }

    // Deterministic labyrinth: 31x31 cells, each cell 6x6 blocks.
    let cells: i32 = 31;
    let half = cells / 2;
    let cell_span: i32 = 6;
    let wall_h = 5;

    for gz in -half..=half {
        for gx in -half..=half {
            let wx = cx + gx * cell_span;
            let wz = cz + gz * cell_span;

            // Every cell gets a small floor patch; avoids accidental holes.
            for dz in -2..=2 {
                for dx in -2..=2 {
                    set_if_inside(chunk, wx + dx, base_y + 2, wz + dz, Block::Dirt);
                }
            }

            // Maze walls with seeded pattern; keep a broad cross corridor.
            let cell_hash = hash3(gx, gz, noise.seed ^ 0x9D33_7AB1);
            let carve_cross = gx.abs() <= 1 || gz.abs() <= 1;
            let wall_here = !carve_cross && (cell_hash & 0x7) >= 2;
            if wall_here {
                for y in base_y + 2..=base_y + 1 + wall_h {
                    for dz in -2..=2 {
                        for dx in -2..=2 {
                            set_if_inside(chunk, wx + dx, y, wz + dz, Block::Wood);
                        }
                    }
                }
            }
        }
    }

    // Four gates into the maze.
    for y in base_y + 2..=base_y + 6 {
        for t in -4..=4 {
            set_if_inside(chunk, cx + t, y, cz - 96, Block::Air);
            set_if_inside(chunk, cx + t, y, cz + 96, Block::Air);
            set_if_inside(chunk, cx - 96, y, cz + t, Block::Air);
            set_if_inside(chunk, cx + 96, y, cz + t, Block::Air);
        }
    }

    // Center marker so it's visible from altitude.
    for y in base_y + 2..=base_y + 12 {
        for z in -2..=2 {
            for x in -2..=2 {
                let block = if y % 2 == 0 { Block::Purple } else { Block::Cyan };
                set_if_inside(chunk, cx + x, y, cz + z, block);
            }
        }
    }
}

pub(super) fn stamp_desert_pyramid(chunk: &mut Chunk, noise: &TerrainNoise) {
    let center = pyramid_center(noise.seed);
    let cx = center.x;
    let cz = center.y;
    let influence = 108;

    let chunk_min_x = chunk.pos.x * CHUNK_SIZE as i32;
    let chunk_max_x = chunk_min_x + CHUNK_SIZE as i32 - 1;
    let chunk_min_z = chunk.pos.y * CHUNK_SIZE as i32;
    let chunk_max_z = chunk_min_z + CHUNK_SIZE as i32 - 1;
    if cx + influence < chunk_min_x
        || cx - influence > chunk_max_x
        || cz + influence < chunk_min_z
        || cz - influence > chunk_max_z
    {
        return;
    }

    let base_y = (sample_surface(noise, cx, cz).height as i32 + 1).clamp(SEA_LEVEL + 4, WORLD_HEIGHT as i32 - 96);
    flatten_village_ground(chunk, cx, base_y, cz, influence);

    // Courtyard.
    for z in -64_i32..=64_i32 {
        for x in -64_i32..=64_i32 {
            let mat = if (x + z).abs() % 5 == 0 { Block::Stone } else { Block::Sand };
            set_if_inside(chunk, cx + x, base_y + 1, cz + z, mat);
        }
    }

    // Main stepped pyramid.
    for step in 0..36 {
        let r = 52 - step;
        let y = base_y + 2 + step;
        for z in -r..=r {
            for x in -r..=r {
                set_if_inside(chunk, cx + x, y, cz + z, Block::Sand);
            }
        }
    }

    // Hollow entry chamber.
    for y in base_y + 3..=base_y + 26 {
        for z in -3..=3 {
            for x in -4..=4 {
                set_if_inside(chunk, cx + x, y, cz - 52 + z, Block::Air);
            }
        }
    }

    // Tall apex temple.
    for y in base_y + 38..=base_y + 52 {
        for z in -8_i32..=8_i32 {
            for x in -8_i32..=8_i32 {
                let border = x.abs() >= 7 || z.abs() >= 7;
                set_if_inside(chunk, cx + x, y, cz + z, if border { Block::Stone } else { Block::Air });
            }
        }
    }
    for y in base_y + 53..=base_y + 66 {
        let r = ((base_y + 66 - y) / 2).clamp(2, 6);
        for z in -r..=r {
            for x in -r..=r {
                set_if_inside(chunk, cx + x, y, cz + z, Block::Sand);
            }
        }
    }

    // Satellite pyramids.
    for (ox, oz) in [(-58, -58), (58, -58), (-58, 58), (58, 58)] {
        for step in 0..12 {
            let r = 14 - step;
            let y = base_y + 2 + step;
            for z in -r..=r {
                for x in -r..=r {
                    set_if_inside(chunk, cx + ox + x, y, cz + oz + z, Block::Sand);
                }
            }
        }
    }

    for y in base_y + 62..=base_y + 72 {
        set_if_inside(chunk, cx, y, cz, Block::Yellow);
    }
}

pub(super) fn stamp_observatory(chunk: &mut Chunk, noise: &TerrainNoise) {
    let center = observatory_center(noise.seed);
    let cx = center.x;
    let cz = center.y;
    let influence = 66;

    let chunk_min_x = chunk.pos.x * CHUNK_SIZE as i32;
    let chunk_max_x = chunk_min_x + CHUNK_SIZE as i32 - 1;
    let chunk_min_z = chunk.pos.y * CHUNK_SIZE as i32;
    let chunk_max_z = chunk_min_z + CHUNK_SIZE as i32 - 1;
    if cx + influence < chunk_min_x
        || cx - influence > chunk_max_x
        || cz + influence < chunk_min_z
        || cz - influence > chunk_max_z
    {
        return;
    }

    let base_y = (sample_surface(noise, cx, cz).height as i32 + 1).clamp(SEA_LEVEL + 14, WORLD_HEIGHT as i32 - 44);
    flatten_village_ground(chunk, cx, base_y, cz, influence);

    // Terraced hill pad.
    for r in [30, 24, 18] {
        for z in -r..=r {
            for x in -r..=r {
                if x * x + z * z <= r * r {
                    set_if_inside(chunk, cx + x, base_y + (30 - r) / 6, cz + z, Block::Stone);
                }
            }
        }
    }

    // Main tower body.
    place_round_tower(chunk, cx, base_y + 2, cz, 8, 16);

    // Dome cap.
    let dome_base = base_y + 19;
    for y in dome_base..=dome_base + 8 {
        let dy = y - dome_base;
        let r = (8 - dy / 2).max(2);
        for z in -r..=r {
            for x in -r..=r {
                if x * x + z * z <= r * r {
                    set_if_inside(chunk, cx + x, y, cz + z, Block::Cyan);
                }
            }
        }
    }

    // Causeway.
    for z in -4..=4 {
        for x in -66..=-8 {
            set_if_inside(chunk, cx + x, base_y + 2, cz + z, Block::Stone);
        }
    }
}

#[inline]
fn monument_center(seed: u32) -> IVec2 {
    let radius = 248.0 + ((seed >> 5) & 127) as f32;
    let angle = ((seed.rotate_left(9) as f32) / (u32::MAX as f32)) * std::f32::consts::TAU;
    IVec2::new((angle.cos() * radius).round() as i32, (angle.sin() * radius).round() as i32)
}

#[inline]
fn city_center(seed: u32) -> IVec2 {
    let monument = monument_center(seed);
    let m = Vec2::new(monument.x as f32, monument.y as f32);
    let mdir = if m.length_squared() > 1.0 {
        m.normalize()
    } else {
        Vec2::new(1.0, 0.0)
    };
    let side = if (seed & 1) == 0 { 1.0 } else { -1.0 };
    let perp = Vec2::new(-mdir.y, mdir.x) * side;
    let outward = mdir * (122.0 + ((seed >> 11) & 63) as f32);
    let lateral = perp * (358.0 + ((seed >> 7) & 95) as f32);
    let c = m + outward + lateral;
    IVec2::new(c.x.round() as i32, c.y.round() as i32)
}

#[inline]
fn maze_center(seed: u32) -> IVec2 {
    let city = city_center(seed);
    let c = Vec2::new(city.x as f32, city.y as f32);
    let dir = if c.length_squared() > 1.0 { c.normalize() } else { Vec2::new(1.0, 0.0) };
    let perp = Vec2::new(-dir.y, dir.x) * if (seed & 2) == 0 { 1.0 } else { -1.0 };
    let p = c + perp * (304.0 + ((seed >> 3) & 95) as f32) - dir * 118.0;
    IVec2::new(p.x.round() as i32, p.y.round() as i32)
}

#[inline]
fn pyramid_center(seed: u32) -> IVec2 {
    let monument = monument_center(seed);
    let m = Vec2::new(monument.x as f32, monument.y as f32);
    let dir = if m.length_squared() > 1.0 { m.normalize() } else { Vec2::new(1.0, 0.0) };
    let p = m * (1.0 + (336.0 + ((seed >> 13) & 127) as f32) / m.length().max(1.0));
    let q = p + Vec2::new(-dir.y, dir.x) * (108.0 + ((seed >> 19) & 63) as f32);
    IVec2::new(q.x.round() as i32, q.y.round() as i32)
}

#[inline]
fn observatory_center(seed: u32) -> IVec2 {
    let city = city_center(seed);
    let c = Vec2::new(city.x as f32, city.y as f32);
    let dir = if c.length_squared() > 1.0 { c.normalize() } else { Vec2::new(1.0, 0.0) };
    let perp = Vec2::new(dir.y, -dir.x);
    let p = c + dir * (256.0 + ((seed >> 17) & 127) as f32) + perp * (124.0 + ((seed >> 21) & 63) as f32);
    IVec2::new(p.x.round() as i32, p.y.round() as i32)
}

fn place_round_tower(chunk: &mut Chunk, cx: i32, base_y: i32, cz: i32, radius: i32, height: i32) {
    for y in base_y..=base_y + height {
        for z in -radius..=radius {
            for x in -radius..=radius {
                let d2 = x * x + z * z;
                if d2 > radius * radius {
                    continue;
                }
                let wx = cx + x;
                let wz = cz + z;
                let shell = d2 >= (radius - 1) * (radius - 1);
                if shell {
                    set_if_inside(chunk, wx, y, wz, Block::Stone);
                } else {
                    set_if_inside(chunk, wx, y, wz, Block::Air);
                }
            }
        }
    }

    let cap_y = base_y + height + 1;
    for y in cap_y..=cap_y + 4 {
        let r = (cap_y + 4 - y).max(1);
        for z in -r..=r {
            for x in -r..=r {
                if x * x + z * z <= r * r {
                    set_if_inside(chunk, cx + x, y, cz + z, Block::Wood);
                }
            }
        }
    }
}

fn place_skyscraper(
    chunk: &mut Chunk,
    cx: i32,
    base_y: i32,
    cz: i32,
    hw: i32,
    hd: i32,
    height: i32,
    seed: u32,
) {
    let shell = if (seed & 1) == 0 { Block::Stone } else { Block::Blue };
    let accent = match (seed >> 2) & 3 {
        0 => Block::Cyan,
        1 => Block::Purple,
        2 => Block::Blue,
        _ => Block::Stone,
    };
    let roof = if (seed & 8) == 0 { Block::Stone } else { Block::Wood };

    for z in -hd - 1..=hd + 1 {
        for x in -hw - 1..=hw + 1 {
            set_if_inside(chunk, cx + x, base_y - 1, cz + z, Block::Stone);
        }
    }

    for y in base_y..=base_y + height {
        for z in -hd..=hd {
            for x in -hw..=hw {
                let wx = cx + x;
                let wz = cz + z;
                let border = x.abs() == hw || z.abs() == hd;
                if border {
                    let mut b = shell;
                    if y % 5 == 0 {
                        b = accent;
                    }
                    // Window bands.
                    if y % 4 == 2 && (x.abs() == hw || z.abs() == hd) {
                        b = Block::Cyan;
                    }
                    set_if_inside(chunk, wx, y, wz, b);
                } else {
                    set_if_inside(chunk, wx, y, wz, Block::Air);
                }
            }
        }
    }

    // Entry doors on opposite sides.
    for y in base_y + 1..=base_y + 3 {
        for x in -1..=1 {
            set_if_inside(chunk, cx + x, y, cz - hd, Block::Air);
            set_if_inside(chunk, cx + x, y, cz + hd, Block::Air);
        }
    }

    // Interior floor plates every few levels, with a wider stair core opening.
    let floor_step = 6;
    for y in ((base_y + 1)..(base_y + height)).step_by(floor_step as usize) {
        for z in -(hd - 1)..=(hd - 1) {
            for x in -(hw - 1)..=(hw - 1) {
                // Keep a 5x5 void in center for stairs/shaft.
                if x.abs() <= 2 && z.abs() <= 2 {
                    continue;
                }
                let mat = if (x + z + y).abs() % 5 == 0 { Block::Stone } else { Block::Wood };
                set_if_inside(chunk, cx + x, y, cz + z, mat);
            }
        }
    }

    // Carve shaft for headroom and cleaner navigation.
    for y in base_y + 1..=base_y + height {
        for z in -2..=2 {
            for x in -2..=2 {
                set_if_inside(chunk, cx + x, y, cz + z, Block::Air);
            }
        }
    }

    // Wider switchback stairs + periodic landings.
    for y in base_y + 1..=base_y + height - 1 {
        let phase = (y - (base_y + 1)).rem_euclid(8);
        let (sx, sz) = match phase {
            0 => (2, -1),
            1 => (2, 0),
            2 => (2, 1),
            3 => (1, 2),
            4 => (0, 2),
            5 => (-1, 2),
            6 => (-2, 1),
            _ => (-2, 0),
        };
        set_if_inside(chunk, cx + sx, y, cz + sz, Block::Stone);
        // Two-wide step to make traversal more forgiving.
        if sx.abs() >= sz.abs() {
            set_if_inside(chunk, cx + sx, y, cz + sz.signum(), Block::Stone);
        } else {
            set_if_inside(chunk, cx + sx.signum(), y, cz + sz, Block::Stone);
        }
        set_if_inside(chunk, cx + sx, y + 1, cz + sz, Block::Air);

        // Landing links from stair to floor plate.
        if (y - (base_y + 1)).rem_euclid(floor_step) == 0 {
            for t in 3..=4 {
                set_if_inside(chunk, cx + sx * t, y, cz + sz * t, Block::Wood);
                set_if_inside(chunk, cx + sx * t, y + 1, cz + sz * t, Block::Air);
            }
        }
    }

    // Roof cap + antenna.
    for z in -hd..=hd {
        for x in -hw..=hw {
            set_if_inside(chunk, cx + x, base_y + height + 1, cz + z, roof);
        }
    }
    for y in base_y + height + 2..=base_y + height + 6 {
        set_if_inside(chunk, cx, y, cz, Block::Yellow);
    }
}

fn flatten_village_ground(chunk: &mut Chunk, cx: i32, ground_y: i32, cz: i32, radius: i32) {
    for dz in -radius..=radius {
        for dx in -radius..=radius {
            if dx * dx + dz * dz > radius * radius + 6 {
                continue;
            }
            let wx = cx + dx;
            let wz = cz + dz;
            if !is_inside_chunk(chunk, wx, wz) {
                continue;
            }
            if let Some(top) = top_solid_y(chunk, wx, wz) {
                if top > ground_y {
                    for y in ground_y + 1..=top {
                        set_if_inside(chunk, wx, y, wz, Block::Air);
                    }
                } else if top < ground_y {
                    for y in top + 1..=ground_y {
                        let fill = if y < ground_y - 1 { Block::Stone } else { Block::Dirt };
                        set_if_inside(chunk, wx, y, wz, fill);
                    }
                }
                if top >= ground_y - 2 {
                    set_if_inside(chunk, wx, ground_y, wz, Block::Grass);
                }
            }
        }
    }
}

fn place_house(chunk: &mut Chunk, cx: i32, base_y: i32, cz: i32, hw: i32, hd: i32, hh: i32, seed: u32) {
    let wall = if (seed & 1) == 0 { Block::Wood } else { Block::Stone };
    let roof = if (seed & 2) == 0 { Block::Wood } else { Block::Stone };
    let accent = if (seed & 4) == 0 { Block::Red } else { Block::Blue };

    for z in -hd..=hd {
        for x in -hw..=hw {
            set_if_inside(chunk, cx + x, base_y - 1, cz + z, Block::Stone);
            set_if_inside(chunk, cx + x, base_y, cz + z, Block::Dirt);
        }
    }

    for y in 1..=hh {
        for z in -hd..=hd {
            for x in -hw..=hw {
                let wx = cx + x;
                let wz = cz + z;
                let on_wall = x.abs() == hw || z.abs() == hd;
                if on_wall {
                    // Door on front wall.
                    if z == -hd && x == 0 && y <= 2 {
                        set_if_inside(chunk, wx, base_y + y, wz, Block::Air);
                    } else if y == 2 && z == -hd && x.abs() == 1 {
                        set_if_inside(chunk, wx, base_y + y, wz, accent);
                    } else {
                        set_if_inside(chunk, wx, base_y + y, wz, wall);
                    }
                } else {
                    set_if_inside(chunk, wx, base_y + y, wz, Block::Air);
                }
            }
        }
    }

    // Simple pitched-ish roof.
    for ry in 0..=2 {
        let shrink = ry;
        for z in -(hd + 1 - shrink)..=(hd + 1 - shrink) {
            for x in -(hw + 1 - shrink)..=(hw + 1 - shrink) {
                set_if_inside(chunk, cx + x, base_y + hh + 1 + ry, cz + z, roof);
            }
        }
    }
}

fn place_boulder(chunk: &mut Chunk, cx: i32, cy: i32, cz: i32, radius: i32, material: Block, seed: u32) {
    for dy in -radius..=radius {
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                let d2 = dx * dx + dy * dy + dz * dz;
                if d2 > radius * radius + (seed as i32 & 1) {
                    continue;
                }
                set_feature_if_air(chunk, cx + dx, cy + dy, cz + dz, material);
            }
        }
    }
}

fn place_reed_clump(chunk: &mut Chunk, x: i32, y: i32, z: i32, seed: u32) {
    let stalks = 2 + ((seed >> 3) % 3) as i32;
    for i in 0..stalks {
        let ox = ((seed >> (i * 2)) as i32 & 1) - (((seed >> (i * 2 + 1)) as i32) & 1);
        let oz = (((seed >> (i * 2 + 4)) as i32) & 1) - (((seed >> (i * 2 + 5)) as i32) & 1);
        let h = 2 + (((seed >> (i * 3 + 8)) % 3) as i32);
        for dy in 0..h {
            set_feature_if_air(chunk, x + ox, y + dy, z + oz, Block::Wood);
        }
        set_feature_if_air(chunk, x + ox, y + h, z + oz, Block::Leaves);
    }
}

#[inline]
fn set_feature_if_air(chunk: &mut Chunk, wx: i32, wy: i32, wz: i32, block: Block) {
    if !(0..WORLD_HEIGHT as i32).contains(&wy) {
        return;
    }
    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let lx = wx - base_x;
    let lz = wz - base_z;
    if lx < 0 || lz < 0 || lx >= CHUNK_SIZE as i32 || lz >= CHUNK_SIZE as i32 {
        return;
    }
    let x = lx as usize;
    let y = wy as usize;
    let z = lz as usize;
    if chunk.get_local(x, y, z) == Block::Air {
        chunk.set_local(x, y, z, block);
    }
}

#[inline]
fn is_inside_chunk(chunk: &Chunk, wx: i32, wz: i32) -> bool {
    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let lx = wx - base_x;
    let lz = wz - base_z;
    lx >= 0 && lz >= 0 && lx < CHUNK_SIZE as i32 && lz < CHUNK_SIZE as i32
}

fn top_solid_y(chunk: &Chunk, wx: i32, wz: i32) -> Option<i32> {
    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let lx = wx - base_x;
    let lz = wz - base_z;
    if lx < 0 || lz < 0 || lx >= CHUNK_SIZE as i32 || lz >= CHUNK_SIZE as i32 {
        return None;
    }
    let x = lx as usize;
    let z = lz as usize;
    for y in (0..WORLD_HEIGHT).rev() {
        let b = chunk.get_local(x, y, z);
        if b != Block::Air {
            return Some(y as i32);
        }
    }
    None
}

fn set_if_inside(chunk: &mut Chunk, wx: i32, wy: i32, wz: i32, block: Block) {
    if !(0..WORLD_HEIGHT as i32).contains(&wy) {
        return;
    }
    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let lx = wx - base_x;
    let lz = wz - base_z;
    if lx < 0 || lz < 0 || lx >= CHUNK_SIZE as i32 || lz >= CHUNK_SIZE as i32 {
        return;
    }
    chunk.set_local(lx as usize, wy as usize, lz as usize, block);
}

fn place_tree(chunk: &mut Chunk, tx: i32, base_y: i32, tz: i32, kind: TreeKind, local_seed: u32) {
    match kind {
        TreeKind::Oak => {
            let trunk_h = 4 + (local_seed % 3) as i32;
            for dy in 0..trunk_h {
                set_in_chunk(chunk, tx, base_y + dy, tz, Block::Wood);
            }
            let top = base_y + trunk_h - 1;
            for dy in -2_i32..=2_i32 {
                let r: i32 = if dy.abs() == 2 { 1 } else { 2 };
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() + dz.abs() > r + 1 {
                            continue;
                        }
                        set_in_chunk(chunk, tx + dx, top + dy, tz + dz, Block::Leaves);
                    }
                }
            }
        }
        TreeKind::Pine => {
            let trunk_h = 6 + (local_seed % 3) as i32;
            for dy in 0..trunk_h {
                set_in_chunk(chunk, tx, base_y + dy, tz, Block::Wood);
            }
            let top = base_y + trunk_h;
            for layer in 0..5 {
                let y = top - layer;
                let r = if layer <= 1 { 1_i32 } else { 2_i32 };
                for dz in -r..=r {
                    for dx in -r..=r {
                        if dx.abs() + dz.abs() > r + 1 {
                            continue;
                        }
                        set_in_chunk(chunk, tx + dx, y, tz + dz, Block::Leaves);
                    }
                }
            }
            set_in_chunk(chunk, tx, top + 1, tz, Block::Leaves);
        }
    }
}

fn set_in_chunk(chunk: &mut Chunk, wx: i32, wy: i32, wz: i32, block: Block) {
    if !(0..WORLD_HEIGHT as i32).contains(&wy) {
        return;
    }
    let base_x = chunk.pos.x * CHUNK_SIZE as i32;
    let base_z = chunk.pos.y * CHUNK_SIZE as i32;
    let lx = wx - base_x;
    let lz = wz - base_z;
    if lx < 0 || lz < 0 || lx >= CHUNK_SIZE as i32 || lz >= CHUNK_SIZE as i32 {
        return;
    }

    let x = lx as usize;
    let y = wy as usize;
    let z = lz as usize;
    let current = chunk.get_local(x, y, z);

    match block {
        Block::Wood => {
            if current == Block::Air || current == Block::Leaves {
                chunk.set_local(x, y, z, Block::Wood);
            }
        }
        Block::Leaves => {
            if current == Block::Air {
                chunk.set_local(x, y, z, Block::Leaves);
            }
        }
        _ => {}
    }
}

#[inline]
fn hash3(x: i32, z: i32, seed: u32) -> u32 {
    let mut h = seed
        ^ (x as u32).wrapping_mul(374_761_393)
        ^ (z as u32).wrapping_mul(668_265_263);
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^ (h >> 16)
}
