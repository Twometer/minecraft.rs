use std::sync::Arc;

use rand::Rng;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::{
    config::ServerConfig,
    world::{sched::GenerationScheduler, BlockPos, ChunkPos, World},
};

#[derive(Clone)]
pub struct Server {
    pub config: Arc<ServerConfig>,
    pub world: Arc<World>,
    pub gen: Arc<GenerationScheduler>,
}

impl Server {
    pub fn new(
        config: Arc<ServerConfig>,
        world: Arc<World>,
        gen: Arc<GenerationScheduler>,
    ) -> Server {
        Server { config, world, gen }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl From<u8> for GameMode {
    fn from(val: u8) -> Self {
        match val {
            0 => GameMode::Survival,
            1 => GameMode::Creative,
            2 => GameMode::Adventure,
            3 => GameMode::Spectator,
            _ => panic!("Invalid game mode {}", val),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2f {
    pub x: f32,
    pub y: f32,
}

pub trait Entity {
    fn id(&self) -> i32;

    fn pos(&self) -> Vec3d;

    fn block_pos(&self) -> BlockPos {
        let pos = self.pos();
        BlockPos::from_pos(pos.x, pos.y, pos.z)
    }

    fn chunk_pos(&self) -> ChunkPos {
        let block_pos = self.block_pos();
        ChunkPos::from_block_pos(block_pos.x, block_pos.z)
    }

    fn set_pos(&mut self, pos: Vec3d);

    fn rot(&self) -> Vec2f;

    fn set_rot(&mut self, rot: Vec2f);
}

pub struct Player {
    pub eid: i32,
    pub uuid: Uuid,
    pub username: String,
    pub position: Vec3d,
    pub rotation: Vec2f,
    pub game_mode: GameMode,
    pub fly_speed: f32,
    pub walk_speed: f32,
}

impl Player {
    pub fn new(eid: i32, game_mode: GameMode) -> Player {
        Player {
            eid,
            uuid: Uuid::from_u128(rand::thread_rng().gen()),
            username: String::new(),
            position: Default::default(),
            rotation: Default::default(),
            game_mode,
            fly_speed: 0.05,
            walk_speed: 0.1,
        }
    }
}

impl Entity for Player {
    fn id(&self) -> i32 {
        self.eid
    }

    fn pos(&self) -> Vec3d {
        self.position
    }

    fn set_pos(&mut self, pos: Vec3d) {
        self.position = pos
    }

    fn rot(&self) -> Vec2f {
        self.rotation
    }

    fn set_rot(&mut self, rot: Vec2f) {
        self.rotation = rot;
    }
}
