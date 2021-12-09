# minecraft.rs

A high-performance Minecraft server in Rust

## Plan

-   Custom Minecraft v1.8 protocol implementation ([see wiki.vg](https://wiki.vg/index.php?title=Protocol&oldid=7121))
-   Asynchronous client handling (Tokio?) for many concurrent players
-   Custom multi-threaded world generator with disk storage
-   Basic Minecraft features
    -   Synchronize player movement and block interaction
    -   Inventory system
    -   Damage system
