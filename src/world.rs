use std::collections::HashMap;

#[derive(PartialEq, Eq, Hash)]
struct ChunkPos {
    x: i32,
    z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> ChunkPos {
        ChunkPos { x, z }
    }

    pub fn from_block_pos(x: i32, z: i32) -> ChunkPos {
        ChunkPos::new(x << 4, z << 4)
    }
}

struct Section {
    data: [u8; 4096],
}

struct Chunk {
    sections: [Option<Section>; 16],
}

struct World {
    chunks: HashMap<ChunkPos, Chunk>,
}
