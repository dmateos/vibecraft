# VibeCraft Terrain Atlas

This is the project-owned terrain atlas used by the engine.

Files:
- `atlas.png`: runtime terrain texture atlas.
- `atlas.xml`: curated block-safe atlas metadata (tile names and coordinates).
- `LICENSE_KENNEY.txt`: original upstream license text.
- `BLOCK_TILE_NAMES.txt`: block-safe tile names currently allowed in terrain atlas metadata.
- `NON_BLOCK_TILE_NAMES.txt`: known sprite/decal/non-block tile names excluded from terrain metadata.

Source provenance:
- Original pack: Kenney Voxel Pack (CC0)
- Source URL: https://opengameart.org/content/voxel-pack-updated

Notes:
- Runtime code should reference this folder, not third-party pack layout paths.
