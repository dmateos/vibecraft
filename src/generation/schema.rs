//! Serializable schema for generation requests and operations.
//! Shared by manual JSON requests, planner output, and live LLM responses so
//! one validation/compiler path can enforce constraints consistently.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationRequest {
    pub version: String,
    pub request_id: String,
    #[serde(default)]
    pub source: String,
    pub ops: Vec<GenerationOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GenerationOp {
    PlaceBlock {
        position: [i32; 3],
        block: String,
    },
    PlacePrefab {
        prefab: String,
        position: [i32; 3],
        #[serde(default)]
        rotation: i32,
        #[serde(default)]
        seed: u32,
    },
    PaintRegion {
        shape: String,
        center: [i32; 2],
        radius: i32,
        surface_block: String,
        #[serde(default)]
        seed: u32,
    },
    FillBox {
        min: [i32; 3],
        max: [i32; 3],
        block: String,
    },
    HollowBox {
        min: [i32; 3],
        max: [i32; 3],
        wall_block: String,
        #[serde(default = "default_wall_thickness")]
        wall_thickness: i32,
        #[serde(default)]
        floor_block: Option<String>,
        #[serde(default)]
        roof_block: Option<String>,
    },
    Cylinder {
        center: [i32; 3],
        radius: i32,
        height: i32,
        block: String,
        #[serde(default)]
        hollow: bool,
    },
    Sphere {
        center: [i32; 3],
        radius: i32,
        block: String,
        #[serde(default)]
        hollow: bool,
    },
    Line {
        from: [i32; 3],
        to: [i32; 3],
        block: String,
        #[serde(default = "default_line_thickness")]
        thickness: i32,
    },
}

fn default_wall_thickness() -> i32 {
    1
}

fn default_line_thickness() -> i32 {
    1
}
