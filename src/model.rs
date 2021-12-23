use std::sync::Arc;

use serde_derive::Deserialize;

use crate::{
    config::ServerConfig,
    world::{sched::GenerationScheduler, World},
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
