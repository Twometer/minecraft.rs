pub mod gen;
mod math;
pub mod sched;

use std::{
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;

#[macro_export]
macro_rules! block_state {
    ($id: expr, $data: expr) => {
        (($id as u16) << 4 | (($data as u16) & 0x0f))
    };
}

#[macro_export]
macro_rules! block_id {
    ($state: expr) => {
        (($state) >> 4)
    };
}

#[macro_export]
macro_rules! block_meta {
    ($state: expr) => {
        (($state) & 0x0f)
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockFace {
    NegY,
    PosY,
    NegZ,
    PosZ,
    NegX,
    PosX,
    Special = 255,
}

impl From<u8> for BlockFace {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::NegY,
            1 => Self::PosY,
            2 => Self::NegZ,
            3 => Self::PosZ,
            4 => Self::NegX,
            5 => Self::PosX,
            255 => Self::Special,
            _ => panic!("Invalid block face"),
        }
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct BlockPos {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[allow(dead_code)]
impl BlockPos {
    pub fn new(x: i32, y: i32, z: i32) -> BlockPos {
        BlockPos { x, y, z }
    }

    pub fn from_pos(x: f64, y: f64, z: f64) -> BlockPos {
        BlockPos {
            x: x.abs() as i32,
            y: y.abs() as i32,
            z: z.abs() as i32,
        }
    }

    pub fn offset(&self, face: BlockFace) -> BlockPos {
        match face {
            BlockFace::PosY => BlockPos::new(self.x, self.y + 1, self.z),
            BlockFace::NegY => BlockPos::new(self.x, self.y - 1, self.z),
            BlockFace::PosZ => BlockPos::new(self.x, self.y, self.z + 1),
            BlockFace::NegZ => BlockPos::new(self.x, self.y, self.z - 1),
            BlockFace::PosX => BlockPos::new(self.x + 1, self.y, self.z),
            BlockFace::NegX => BlockPos::new(self.x - 1, self.y, self.z),
            _ => panic!("Cannot offset BlockPos by {:?}", face),
        }
    }

    pub fn to_u64(&self) -> u64 {
        let x = self.x as u64;
        let y = self.y as u64;
        let z = self.z as u64;
        ((x & 0x3FFFFFF) << 38) | ((y & 0xFFF) << 26) | (z & 0x3FFFFFF)
    }

    fn to_signed(val: u64, bits: u32) -> i32 {
        let mut val = val as i32;
        if val >= i32::pow(2, bits - 1) {
            val -= i32::pow(2, bits);
        }
        val
    }
}

impl From<u64> for BlockPos {
    fn from(value: u64) -> Self {
        BlockPos {
            x: Self::to_signed(value >> 38, 26),
            y: Self::to_signed((value >> 26) & 0xFFF, 12),
            z: Self::to_signed(value << 38 >> 38, 26),
        }
    }
}

#[derive(PartialEq, Eq, Hash, Clone, Copy, Debug)]
pub struct ChunkPos {
    pub x: i32,
    pub z: i32,
}

impl ChunkPos {
    pub fn new(x: i32, z: i32) -> ChunkPos {
        ChunkPos { x, z }
    }

    pub fn from_block_pos(x: i32, z: i32) -> ChunkPos {
        ChunkPos::new(x >> 4, z >> 4)
    }
}

#[derive(Clone, Debug)]
pub struct Section {
    pub data: [u16; 4096],
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

#[derive(Clone, Debug)]
pub struct Chunk {
    pub x: i32,
    pub z: i32,
    pub sections: [Option<Section>; 16],
    pub biomes: [u8; 256],
}

impl Chunk {
    fn new(x: i32, z: i32) -> Chunk {
        Chunk {
            x,
            z,
            sections: Default::default(),
            biomes: [0; 256],
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

    pub fn set_block_if_air(&mut self, x: i32, y: i32, z: i32, block_state: u16) {
        let section_idx = y >> 4;
        let mut section_opt = &mut self.sections[section_idx as usize];
        if section_opt.is_none() {
            self.sections[section_idx as usize] = Some(Section::new());
            section_opt = &mut self.sections[section_idx as usize];
            section_opt
                .as_mut()
                .unwrap()
                .set_block(x, y & 0x0f, z, block_state);
            return;
        }

        let section = section_opt.as_mut().unwrap();
        if section.get_block(x, y & 0x0f, z) == 0 {
            section.set_block(x, y & 0x0f, z, block_state);
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

    pub fn set_biome(&mut self, x: i32, z: i32, biome: u8) {
        self.biomes[(z * 16 + x) as usize] = biome;
    }
}

pub type MutexChunkRef = Arc<Mutex<Chunk>>;

pub struct World {
    chunks: DashMap<ChunkPos, MutexChunkRef>,
}

#[allow(dead_code)]
impl World {
    pub fn new() -> World {
        World {
            chunks: DashMap::with_capacity(256),
        }
    }

    pub fn has_chunk(&self, pos: ChunkPos) -> bool {
        self.chunks.contains_key(&pos)
    }

    pub fn get_chunk(&self, pos: ChunkPos) -> Option<MutexChunkRef> {
        match self.chunks.get(&pos) {
            Some(chunk_ref) => Some(chunk_ref.clone()),
            None => None,
        }
    }

    pub fn create_chunk(&self, pos: ChunkPos) -> MutexChunkRef {
        match self.chunks.get(&pos) {
            Some(chunk_ref) => chunk_ref.clone(),
            None => {
                let new_chunk = Arc::new(Mutex::new(Chunk::new(pos.x, pos.z)));
                self.chunks.insert(pos, new_chunk.clone());
                new_chunk
            }
        }
    }

    pub fn insert_chunk(&self, chunk: Chunk) {
        self.chunks
            .insert(ChunkPos::new(chunk.x, chunk.z), Arc::new(Mutex::new(chunk)));
    }

    pub fn get_block(&self, x: i32, y: i32, z: i32) -> u16 {
        let chunk_opt = self.get_chunk(ChunkPos::from_block_pos(x, z));
        match chunk_opt {
            Some(chunk) => chunk.lock().unwrap().get_block(x & 0x0f, y, z & 0x0f),
            None => 0,
        }
    }

    pub fn set_block(&self, x: i32, y: i32, z: i32, block_state: u16) {
        let chunk = self.create_chunk(ChunkPos::from_block_pos(x, z));
        chunk
            .lock()
            .unwrap()
            .set_block(x & 0x0f, y, z & 0x0f, block_state);
    }
}

pub fn random_seed() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Failed to get UNIX time")
        .as_secs() as u32
}
