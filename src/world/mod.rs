pub mod gen;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

#[macro_export]
macro_rules! block_state {
    ($id: expr, $data: expr) => {
        ($id << 4 | ($data & 0x0f))
    };
}

#[derive(PartialEq, Eq, Hash, Clone, Copy)]
pub struct ChunkPos {
    x: i32,
    z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> ChunkPos {
        ChunkPos { x, z }
    }

    pub fn from_block_pos(x: i32, z: i32) -> ChunkPos {
        ChunkPos::new(x >> 4, z >> 4)
    }
}

pub struct Section {
    data: [u16; 4096],
}

impl Section {
    fn new() -> Section {
        Section { data: [0; 4096] }
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> u16 {
        if x < 0 || y < 0 || z < 0 || x > 15 || y > 15 || z > 15 {
            return 0;
        }

        let block_idx = x + 16 * (z + 16 * y);
        self.data[block_idx as usize]
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_state: u16) {
        if x < 0 || y < 0 || z < 0 || x > 15 || y > 15 || z > 15 {
            return;
        }
        let block_idx = x + 16 * (z + 16 * y);
        self.data[block_idx as usize] = block_state
    }
}

pub struct Chunk {
    sections: [Option<Section>; 16],
}

impl Chunk {
    fn new() -> Chunk {
        Chunk {
            sections: Default::default(),
        }
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> u16 {
        let section_idx = y >> 4;
        let section_opt = &self.sections[section_idx as usize];
        match section_opt {
            Some(section) => section.get_block(x, y & 0x0f, z),
            None => 0,
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_state: u16) {
        let section_idx = y >> 4;
        let mut section_opt = &mut self.sections[section_idx as usize];
        if section_opt.is_none() {
            self.sections[section_idx as usize] = Some(Section::new());
            section_opt = &mut self.sections[section_idx as usize];
        }

        section_opt
            .as_mut()
            .unwrap()
            .set_block(x, y & 0x0f, z, block_state)
    }
}

type MutexChunkRef = Arc<Mutex<Chunk>>;

pub struct World {
    chunks: HashMap<ChunkPos, MutexChunkRef>,
}

impl World {
    pub fn new() -> World {
        World {
            chunks: HashMap::new(),
        }
    }

    pub fn get_chunk(&self, pos: ChunkPos) -> Option<MutexChunkRef> {
        match self.chunks.get(&pos) {
            Some(chk) => Some(chk.clone()),
            None => None,
        }
    }

    pub fn create_chunk(&mut self, pos: ChunkPos) -> MutexChunkRef {
        if !self.chunks.contains_key(&pos) {
            self.chunks.insert(pos, Arc::new(Mutex::new(Chunk::new())));
        }

        self.chunks[&pos].clone()
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> u16 {
        let chunk_opt = self.get_chunk(ChunkPos::from_block_pos(x, z));
        match chunk_opt {
            Some(chunk) => chunk.lock().unwrap().get_block(x & 0x0f, y, z & 0x0f),
            None => 0,
        }
    }

    pub fn set_block(&mut self, x: i32, y: i32, z: i32, block_state: u16) {
        let chunk = self.create_chunk(ChunkPos::from_block_pos(x, z));
        chunk
            .lock()
            .unwrap()
            .set_block(x & 0x0f, y, z & 0x0f, block_state);
    }
}
