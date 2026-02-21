//! Validation and guardrails for generation request payloads.
//! Rejects unsupported/unsafe ops early so downstream compilation and world
//! editing run against bounded, predictable, and debuggable input.
use crate::generation::schema::{GenerationOp, GenerationRequest};

pub fn validate_request(req: &GenerationRequest) -> Result<(), String> {
    if req.version != "1" {
        return Err(format!("unsupported version: {}", req.version));
    }
    if req.request_id.trim().is_empty() {
        return Err("request_id cannot be empty".to_string());
    }
    if req.ops.is_empty() {
        return Err("ops cannot be empty".to_string());
    }
    if req.ops.len() > 128 {
        return Err("too many ops; max is 128".to_string());
    }

    for (idx, op) in req.ops.iter().enumerate() {
        match op {
            GenerationOp::PlaceBlock { block, .. } => {
                if block.trim().is_empty() {
                    return Err(format!("op[{idx}] block cannot be empty"));
                }
            }
            GenerationOp::PlacePrefab {
                prefab,
                rotation,
                ..
            } => {
                if prefab.trim().is_empty() {
                    return Err(format!("op[{idx}] prefab cannot be empty"));
                }
                if rotation.rem_euclid(90) != 0 {
                    return Err(format!("op[{idx}] rotation must be a multiple of 90"));
                }
            }
            GenerationOp::PaintRegion {
                shape,
                radius,
                surface_block,
                ..
            } => {
                if shape != "circle" {
                    return Err(format!("op[{idx}] only shape='circle' is supported"));
                }
                if *radius <= 0 || *radius > 96 {
                    return Err(format!("op[{idx}] radius must be in 1..=96"));
                }
                if surface_block.trim().is_empty() {
                    return Err(format!("op[{idx}] surface_block cannot be empty"));
                }
            }
            GenerationOp::FillBox { min, max, block } => {
                if block.trim().is_empty() {
                    return Err(format!("op[{idx}] block cannot be empty"));
                }
                validate_box_bounds(idx, *min, *max)?;
            }
            GenerationOp::HollowBox {
                min,
                max,
                wall_block,
                wall_thickness,
                floor_block,
                roof_block,
            } => {
                if wall_block.trim().is_empty() {
                    return Err(format!("op[{idx}] wall_block cannot be empty"));
                }
                if *wall_thickness <= 0 || *wall_thickness > 8 {
                    return Err(format!("op[{idx}] wall_thickness must be in 1..=8"));
                }
                validate_box_bounds(idx, *min, *max)?;
                if let Some(f) = floor_block
                    && f.trim().is_empty()
                {
                    return Err(format!("op[{idx}] floor_block cannot be empty"));
                }
                if let Some(r) = roof_block
                    && r.trim().is_empty()
                {
                    return Err(format!("op[{idx}] roof_block cannot be empty"));
                }
            }
            GenerationOp::Cylinder {
                radius,
                height,
                block,
                ..
            } => {
                if block.trim().is_empty() {
                    return Err(format!("op[{idx}] block cannot be empty"));
                }
                if *radius <= 0 || *radius > 64 {
                    return Err(format!("op[{idx}] radius must be in 1..=64"));
                }
                if *height <= 0 || *height > 160 {
                    return Err(format!("op[{idx}] height must be in 1..=160"));
                }
            }
            GenerationOp::Sphere {
                radius,
                block,
                ..
            } => {
                if block.trim().is_empty() {
                    return Err(format!("op[{idx}] block cannot be empty"));
                }
                if *radius <= 0 || *radius > 96 {
                    return Err(format!("op[{idx}] radius must be in 1..=96"));
                }
            }
            GenerationOp::Line {
                from,
                to,
                block,
                thickness,
            } => {
                if block.trim().is_empty() {
                    return Err(format!("op[{idx}] block cannot be empty"));
                }
                if *thickness <= 0 || *thickness > 8 {
                    return Err(format!("op[{idx}] thickness must be in 1..=8"));
                }
                let dx = (to[0] - from[0]).abs();
                let dy = (to[1] - from[1]).abs();
                let dz = (to[2] - from[2]).abs();
                let len = dx.max(dy).max(dz);
                if len > 512 {
                    return Err(format!("op[{idx}] line too long ({len}, max 512)"));
                }
            }
        }
    }

    Ok(())
}

fn validate_box_bounds(idx: usize, min: [i32; 3], max: [i32; 3]) -> Result<(), String> {
    let sx = max[0] - min[0] + 1;
    let sy = max[1] - min[1] + 1;
    let sz = max[2] - min[2] + 1;
    if sx <= 0 || sy <= 0 || sz <= 0 {
        return Err(format!("op[{idx}] box bounds must satisfy min <= max on all axes"));
    }
    if sx > 192 || sy > 192 || sz > 192 {
        return Err(format!("op[{idx}] box axis length too large (max 192)"));
    }
    let volume = sx as i64 * sy as i64 * sz as i64;
    if volume > 300_000 {
        return Err(format!("op[{idx}] box volume too large ({volume}, max 300000)"));
    }
    Ok(())
}
