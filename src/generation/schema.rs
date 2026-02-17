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
}
