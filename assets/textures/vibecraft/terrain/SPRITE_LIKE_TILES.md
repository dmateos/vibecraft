# Sprite-Like / Non-Tiling Tile Candidates

These tiles are likely to look wrong on full voxel cube faces (transparent edges, billboard-style art, or decorative decals):

- `grass1.png`
- `grass2.png`
- `grass3.png`
- `grass4.png`
- `leaves_transparent.png`
- `leaves_orange_transparent.png`
- `glass_frame.png`
- `track_corner.png`
- `track_corner_alt.png`
- `track_straight.png`
- `track_straight_alt.png`
- `wheat_stage1.png`
- `wheat_stage2.png`
- `wheat_stage3.png`
- `wheat_stage4.png`

## Notes
- Current terrain/block face mapping in code uses only block-safe tiling textures.
- If any of the candidates above are used in the future, they should be rendered as sprites/meshes, not solid cube faces.
