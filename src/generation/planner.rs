use crate::generation::schema::{GenerationOp, GenerationRequest};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PromptIntent {
    Pyramid,
    Tower,
    House,
    HauntedHouse,
    Bridge,
    Castle,
    Church,
    Spaceship,
    Unknown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum BuildScale {
    Small,
    Medium,
    Large,
    Massive,
}

#[derive(Debug, Clone, Copy)]
struct BuildSpec {
    intent: PromptIntent,
    scale: BuildScale,
    fancy: bool,
    y_base: i32,
    anchor: [i32; 3],
    seed: u32,
}

pub fn build_structured_plan(prompt: &str, anchor: [i32; 3], seed: u32) -> Option<GenerationRequest> {
    let prompt_lc = prompt.to_lowercase();
    let intent = detect_intent(&prompt_lc);
    if intent == PromptIntent::Unknown {
        return None;
    }

    let spec = BuildSpec {
        intent,
        scale: detect_scale(&prompt_lc),
        fancy: contains_any(
            &prompt_lc,
            &["fancy", "ornate", "ornament", "detailed", "grand", "gothic", "beautiful"],
        ),
        y_base: anchor[1].max(4),
        anchor,
        seed,
    };

    Some(compile_spec(spec))
}

fn compile_spec(spec: BuildSpec) -> GenerationRequest {
    let ops = match spec.intent {
        PromptIntent::Pyramid => compile_pyramid(spec),
        PromptIntent::Tower => compile_tower(spec),
        PromptIntent::House => compile_house(spec),
        PromptIntent::HauntedHouse => compile_haunted_house(spec),
        PromptIntent::Bridge => compile_bridge(spec),
        PromptIntent::Castle => compile_castle(spec),
        PromptIntent::Church => compile_church(spec),
        PromptIntent::Spaceship => compile_spaceship(spec),
        PromptIntent::Unknown => Vec::new(),
    };

    GenerationRequest {
        version: "1".to_string(),
        request_id: format!("local-{}-{}", intent_name(spec.intent), spec.seed),
        source: "local_passes".to_string(),
        ops,
    }
}

fn detect_intent(prompt_lc: &str) -> PromptIntent {
    if contains_any(prompt_lc, &["pyramid", "ziggurat"]) {
        return PromptIntent::Pyramid;
    }
    if contains_any(prompt_lc, &["haunted house", "haunted", "spooky house", "manor"]) {
        return PromptIntent::HauntedHouse;
    }
    if contains_any(prompt_lc, &["spaceship", "space ship", "ufo", "starship", "alien ship"]) {
        return PromptIntent::Spaceship;
    }
    if contains_any(prompt_lc, &["cathedral", "church", "chapel", "basilica"]) {
        return PromptIntent::Church;
    }
    if contains_any(prompt_lc, &["castle", "fortress", "citadel", "keep"]) {
        return PromptIntent::Castle;
    }
    if contains_any(prompt_lc, &["bridge", "viaduct"]) {
        return PromptIntent::Bridge;
    }
    if contains_any(prompt_lc, &["house", "home", "cabin"]) {
        return PromptIntent::House;
    }
    if contains_any(prompt_lc, &["tower", "obelisk", "spire"]) {
        return PromptIntent::Tower;
    }
    PromptIntent::Unknown
}

fn detect_scale(prompt_lc: &str) -> BuildScale {
    if contains_any(prompt_lc, &["massive", "huge", "giant", "enormous", "mega"]) {
        return BuildScale::Massive;
    }
    if contains_any(prompt_lc, &["large", "big"]) {
        return BuildScale::Large;
    }
    if contains_any(prompt_lc, &["small", "tiny", "compact"]) {
        return BuildScale::Small;
    }
    BuildScale::Medium
}

fn compile_pyramid(spec: BuildSpec) -> Vec<GenerationOp> {
    let (base, height) = match spec.scale {
        BuildScale::Small => (14, 9),
        BuildScale::Medium => (24, 14),
        BuildScale::Large => (34, 20),
        BuildScale::Massive => (52, 30),
    };
    let cx = spec.anchor[0];
    let cz = spec.anchor[2];
    let mut ops = Vec::new();
    let mut current_base = base;
    let mut y = spec.y_base;
    while current_base > 1 && y < spec.y_base + height {
        let half = current_base / 2;
        ops.push(GenerationOp::FillBox {
            min: [cx - half, y, cz - half],
            max: [cx + half, y, cz + half],
            block: "sand".to_string(),
        });
        y += 1;
        current_base -= 2;
    }
    if spec.fancy {
        ops.push(GenerationOp::Cylinder {
            center: [cx, spec.y_base + height - 2, cz],
            radius: 2,
            height: 3,
            block: "yellow".to_string(),
            hollow: false,
        });
    }
    ops
}

fn compile_tower(spec: BuildSpec) -> Vec<GenerationOp> {
    let (radius, height) = match spec.scale {
        BuildScale::Small => (3, 14),
        BuildScale::Medium => (4, 22),
        BuildScale::Large => (6, 34),
        BuildScale::Massive => (8, 52),
    };
    let cx = spec.anchor[0];
    let cz = spec.anchor[2];
    let mut ops = vec![GenerationOp::Cylinder {
        center: [cx, spec.y_base, cz],
        radius,
        height,
        block: "castle_stone".to_string(),
        hollow: true,
    }];
    if spec.fancy {
        ops.push(GenerationOp::Cylinder {
            center: [cx, spec.y_base + height, cz],
            radius: radius / 2 + 1,
            height: 5,
            block: "castle_trim".to_string(),
            hollow: false,
        });
    }
    ops
}

fn compile_house(spec: BuildSpec) -> Vec<GenerationOp> {
    let (w, d, h) = match spec.scale {
        BuildScale::Small => (12, 10, 7),
        BuildScale::Medium => (18, 14, 9),
        BuildScale::Large => (24, 18, 11),
        BuildScale::Massive => (34, 24, 14),
    };
    let cx = spec.anchor[0];
    let cz = spec.anchor[2];
    let mut ops = vec![
        GenerationOp::HollowBox {
            min: [cx - w / 2, spec.y_base, cz - d / 2],
            max: [cx + w / 2, spec.y_base + h, cz + d / 2],
            wall_block: "wood".to_string(),
            wall_thickness: 1,
            floor_block: Some("castle_floor".to_string()),
            roof_block: None,
        },
        GenerationOp::FillBox {
            min: [cx - w / 2 - 1, spec.y_base + h + 1, cz - d / 2 - 1],
            max: [cx + w / 2 + 1, spec.y_base + h + 1, cz + d / 2 + 1],
            block: "roof_dark".to_string(),
        },
    ];
    if spec.fancy {
        ops.push(GenerationOp::Cylinder {
            center: [cx + w / 2 - 2, spec.y_base, cz - d / 2 + 2],
            radius: 2,
            height: h + 4,
            block: "stone".to_string(),
            hollow: true,
        });
    }
    ops
}

fn compile_haunted_house(spec: BuildSpec) -> Vec<GenerationOp> {
    let (w, d, h, spire_h) = match spec.scale {
        BuildScale::Small => (14, 12, 9, 10),
        BuildScale::Medium => (22, 16, 13, 14),
        BuildScale::Large => (30, 22, 17, 18),
        BuildScale::Massive => (40, 30, 23, 24),
    };
    let cx = spec.anchor[0];
    let cz = spec.anchor[2];
    let y = spec.y_base;

    let mut ops = vec![
        GenerationOp::HollowBox {
            min: [cx - w / 2, y, cz - d / 2],
            max: [cx + w / 2, y + h, cz + d / 2],
            wall_block: "stone".to_string(),
            wall_thickness: 1,
            floor_block: Some("castle_floor".to_string()),
            roof_block: Some("roof_dark".to_string()),
        },
        GenerationOp::HollowBox {
            min: [cx - (w / 2 - 3), y + h - 2, cz - (d / 2 - 3)],
            max: [cx + (w / 2 - 3), y + h + 5, cz + (d / 2 - 3)],
            wall_block: "wood".to_string(),
            wall_thickness: 1,
            floor_block: None,
            roof_block: Some("roof_dark".to_string()),
        },
        GenerationOp::FillBox {
            min: [cx - 4, y + 1, cz - d / 2 - 3],
            max: [cx + 4, y + 1, cz - d / 2],
            block: "wood".to_string(),
        },
        GenerationOp::Cylinder {
            center: [cx - w / 2 + 2, y + h - 1, cz - d / 2 + 2],
            radius: 2,
            height: spire_h,
            block: "stone".to_string(),
            hollow: true,
        },
        GenerationOp::Cylinder {
            center: [cx + w / 2 - 2, y + h - 1, cz - d / 2 + 2],
            radius: 2,
            height: spire_h,
            block: "stone".to_string(),
            hollow: true,
        },
    ];

    if spec.fancy {
        ops.push(GenerationOp::FillBox {
            min: [cx - 1, y + 3, cz - d / 2],
            max: [cx + 1, y + h - 2, cz - d / 2],
            block: "purple".to_string(),
        });
        ops.push(GenerationOp::FillBox {
            min: [cx - 1, y + 3, cz + d / 2],
            max: [cx + 1, y + h - 2, cz + d / 2],
            block: "cyan".to_string(),
        });
    }

    ops
}

fn compile_bridge(spec: BuildSpec) -> Vec<GenerationOp> {
    let (len, width, deck_h) = match spec.scale {
        BuildScale::Small => (24, 4, 6),
        BuildScale::Medium => (38, 6, 8),
        BuildScale::Large => (56, 8, 11),
        BuildScale::Massive => (78, 10, 14),
    };
    let cx = spec.anchor[0];
    let cz = spec.anchor[2];
    let y = spec.y_base + 2;
    let mut ops = vec![
        GenerationOp::FillBox {
            min: [cx - len / 2, y + deck_h, cz - width / 2],
            max: [cx + len / 2, y + deck_h + 1, cz + width / 2],
            block: "castle_floor".to_string(),
        },
        GenerationOp::FillBox {
            min: [cx - len / 2, y + deck_h + 2, cz - width / 2],
            max: [cx + len / 2, y + deck_h + 2, cz - width / 2],
            block: "castle_trim".to_string(),
        },
        GenerationOp::FillBox {
            min: [cx - len / 2, y + deck_h + 2, cz + width / 2],
            max: [cx + len / 2, y + deck_h + 2, cz + width / 2],
            block: "castle_trim".to_string(),
        },
    ];

    for x in [cx - len / 2 + 2, cx - len / 6, cx + len / 6, cx + len / 2 - 2] {
        ops.push(GenerationOp::Cylinder {
            center: [x, y, cz],
            radius: 2,
            height: deck_h + 2,
            block: "stone".to_string(),
            hollow: false,
        });
    }
    ops
}

fn compile_castle(spec: BuildSpec) -> Vec<GenerationOp> {
    let (keep, wall, keep_h, wall_h, tower_r, tower_h) = match spec.scale {
        BuildScale::Small => (16, 32, 12, 8, 3, 14),
        BuildScale::Medium => (22, 44, 16, 10, 4, 18),
        BuildScale::Large => (30, 60, 22, 12, 5, 24),
        BuildScale::Massive => (42, 88, 30, 15, 7, 34),
    };
    let cx = spec.anchor[0];
    let cz = spec.anchor[2];
    let y = spec.y_base;
    let wh = wall / 2;
    let kh = keep / 2;

    let mut ops = vec![
        GenerationOp::HollowBox {
            min: [cx - kh, y, cz - kh],
            max: [cx + kh, y + keep_h, cz + kh],
            wall_block: "castle_stone".to_string(),
            wall_thickness: 1,
            floor_block: Some("castle_floor".to_string()),
            roof_block: Some("castle_trim".to_string()),
        },
        GenerationOp::HollowBox {
            min: [cx - wh, y, cz - wh],
            max: [cx + wh, y + wall_h, cz + wh],
            wall_block: "castle_stone".to_string(),
            wall_thickness: 1,
            floor_block: Some("castle_floor".to_string()),
            roof_block: None,
        },
    ];

    for (tx, tz) in [
        (cx - wh, cz - wh),
        (cx + wh, cz - wh),
        (cx - wh, cz + wh),
        (cx + wh, cz + wh),
    ] {
        ops.push(GenerationOp::Cylinder {
            center: [tx, y, tz],
            radius: tower_r,
            height: tower_h,
            block: "castle_trim".to_string(),
            hollow: true,
        });
    }

    if spec.fancy {
        ops.push(GenerationOp::FillBox {
            min: [cx - 2, y + keep_h + 1, cz - 2],
            max: [cx + 2, y + keep_h + 4, cz + 2],
            block: "banner_warm".to_string(),
        });
    }

    ops
}

fn compile_church(spec: BuildSpec) -> Vec<GenerationOp> {
    let (nave_l, nave_w, nave_h, transept_w, transept_l, tower_r, tower_h) = match spec.scale {
        BuildScale::Small => (30, 10, 12, 20, 8, 3, 18),
        BuildScale::Medium => (44, 14, 16, 30, 10, 4, 24),
        BuildScale::Large => (62, 18, 22, 40, 12, 5, 32),
        BuildScale::Massive => (88, 24, 30, 58, 16, 7, 46),
    };

    let cx = spec.anchor[0];
    let cz = spec.anchor[2];
    let y = spec.y_base;

    let mut ops = vec![
        GenerationOp::HollowBox {
            min: [cx - nave_l / 2, y, cz - nave_w / 2],
            max: [cx + nave_l / 2, y + nave_h, cz + nave_w / 2],
            wall_block: "castle_stone".to_string(),
            wall_thickness: 1,
            floor_block: Some("castle_floor".to_string()),
            roof_block: Some("roof_dark".to_string()),
        },
        GenerationOp::HollowBox {
            min: [cx - transept_l / 2, y + 1, cz - transept_w / 2],
            max: [cx + transept_l / 2, y + nave_h - 2, cz + transept_w / 2],
            wall_block: "castle_trim".to_string(),
            wall_thickness: 1,
            floor_block: Some("castle_floor".to_string()),
            roof_block: Some("roof_dark".to_string()),
        },
        GenerationOp::Cylinder {
            center: [cx + nave_l / 2 - 2, y, cz],
            radius: nave_w / 2,
            height: nave_h - 2,
            block: "castle_trim".to_string(),
            hollow: true,
        },
        GenerationOp::Cylinder {
            center: [cx - nave_l / 2 + 2, y, cz - nave_w / 2 - 2],
            radius: tower_r,
            height: tower_h,
            block: "castle_stone".to_string(),
            hollow: true,
        },
        GenerationOp::Cylinder {
            center: [cx - nave_l / 2 + 2, y, cz + nave_w / 2 + 2],
            radius: tower_r,
            height: tower_h,
            block: "castle_stone".to_string(),
            hollow: true,
        },
    ];

    if spec.fancy {
        ops.push(GenerationOp::Cylinder {
            center: [cx, y + nave_h - 1, cz],
            radius: tower_r.saturating_sub(1).max(2),
            height: tower_h / 2,
            block: "castle_trim".to_string(),
            hollow: false,
        });
        ops.push(GenerationOp::FillBox {
            min: [cx - 1, y + 4, cz - nave_w / 2],
            max: [cx + 1, y + nave_h - 3, cz - nave_w / 2],
            block: "banner_cool".to_string(),
        });
        ops.push(GenerationOp::FillBox {
            min: [cx - 1, y + 4, cz + nave_w / 2],
            max: [cx + 1, y + nave_h - 3, cz + nave_w / 2],
            block: "banner_warm".to_string(),
        });
    }

    ops
}

fn compile_spaceship(spec: BuildSpec) -> Vec<GenerationOp> {
    let (body_r, body_len, fin_h, leg_h) = match spec.scale {
        BuildScale::Small => (5, 16, 5, 4),
        BuildScale::Medium => (8, 24, 8, 6),
        BuildScale::Large => (12, 36, 11, 8),
        BuildScale::Massive => (16, 52, 15, 10),
    };
    let cx = spec.anchor[0];
    let cz = spec.anchor[2];
    let y = spec.y_base + leg_h + 2;

    let mut ops = vec![
        GenerationOp::Sphere {
            center: [cx, y, cz],
            radius: body_r,
            block: "stone".to_string(),
            hollow: false,
        },
        GenerationOp::Sphere {
            center: [cx + body_len / 4, y, cz],
            radius: body_r - 1,
            block: "castle_trim".to_string(),
            hollow: false,
        },
        GenerationOp::Sphere {
            center: [cx - body_len / 4, y, cz],
            radius: body_r - 1,
            block: "castle_trim".to_string(),
            hollow: false,
        },
        GenerationOp::Sphere {
            center: [cx, y + body_r - 2, cz],
            radius: (body_r / 2).max(3),
            block: "banner_cool".to_string(),
            hollow: false,
        },
        GenerationOp::Line {
            from: [cx - body_len / 2, y, cz],
            to: [cx + body_len / 2, y, cz],
            block: "castle_trim".to_string(),
            thickness: 2,
        },
        GenerationOp::Line {
            from: [cx, y, cz - body_len / 3],
            to: [cx, y, cz + body_len / 3],
            block: "castle_trim".to_string(),
            thickness: 2,
        },
        GenerationOp::Line {
            from: [cx - body_r, y - body_r + 1, cz - body_r],
            to: [cx - body_r - 2, spec.y_base, cz - body_r - 2],
            block: "stone".to_string(),
            thickness: 1,
        },
        GenerationOp::Line {
            from: [cx + body_r, y - body_r + 1, cz - body_r],
            to: [cx + body_r + 2, spec.y_base, cz - body_r - 2],
            block: "stone".to_string(),
            thickness: 1,
        },
        GenerationOp::Line {
            from: [cx - body_r, y - body_r + 1, cz + body_r],
            to: [cx - body_r - 2, spec.y_base, cz + body_r + 2],
            block: "stone".to_string(),
            thickness: 1,
        },
        GenerationOp::Line {
            from: [cx + body_r, y - body_r + 1, cz + body_r],
            to: [cx + body_r + 2, spec.y_base, cz + body_r + 2],
            block: "stone".to_string(),
            thickness: 1,
        },
    ];

    if spec.fancy {
        ops.push(GenerationOp::Line {
            from: [cx - body_len / 2, y + 1, cz + fin_h],
            to: [cx + body_len / 2, y + 1, cz + fin_h],
            block: "red".to_string(),
            thickness: 1,
        });
        ops.push(GenerationOp::Line {
            from: [cx - body_len / 2, y + 1, cz - fin_h],
            to: [cx + body_len / 2, y + 1, cz - fin_h],
            block: "blue".to_string(),
            thickness: 1,
        });
    }

    ops
}

fn intent_name(intent: PromptIntent) -> &'static str {
    match intent {
        PromptIntent::Pyramid => "pyramid",
        PromptIntent::Tower => "tower",
        PromptIntent::House => "house",
        PromptIntent::HauntedHouse => "haunted_house",
        PromptIntent::Bridge => "bridge",
        PromptIntent::Castle => "castle",
        PromptIntent::Church => "church",
        PromptIntent::Spaceship => "spaceship",
        PromptIntent::Unknown => "unknown",
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}
