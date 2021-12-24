# minecraft.rs

A high-performance Minecraft server in Rust

## Feature plan

-   [x] Minecraft 1.8 protocol implementation ([wiki.vg](https://wiki.vg/index.php?title=Protocol&oldid=7121))
-   [x] Asynchronous client handling with Tokio for many concurrent players
-   [x] Custom multithreaded world generation engine
    -   [x] Configurable biome generation
        -   Per-biome heightmap config
        -   Per-biome feature distribution (grass, trees, ...)
    -   [x] Configurable ore generation
    -   [x] Configurable cave generation
-   [ ] Basic Minecraft features
    -   [ ] Synchronize player movement and block interaction
    -   [ ] Inventory & crafting system
    -   [ ] Animal spawning and AI
    -   [ ] Damage system
