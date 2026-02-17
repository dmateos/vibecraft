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
        }
    }

    Ok(())
}
