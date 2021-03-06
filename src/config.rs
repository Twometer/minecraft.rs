use std::{collections::HashMap, fs};

use serde_derive::Deserialize;

use crate::model::GameMode;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
pub enum BiomeLayer {
    Sea,
    River,
    Land,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BiomeConfig {
    pub id: u8,
    pub temperature: Option<f64>,
    pub elevation: Option<f64>,
    pub moisture: Option<f64>,
    pub scale: f64,
    pub layer: BiomeLayer,
    #[serde(default)]
    pub sea_level: bool,
    pub blocks: Vec<u8>,
    pub surface_layer: Option<u8>,
    #[serde(default)]
    pub features: HashMap<String, f64>,
}

#[derive(Debug, Deserialize)]
pub struct OreConfig {
    pub id: u8,
    pub center: f64,
    pub spread: f64,
    pub scale: f64,
    pub threshold: f64,
}

#[derive(Debug, Deserialize)]
pub struct WorldGenConfig {
    pub master_scale: f64,
    pub ocean_level: f64,
    pub biome_smoothing: i32,
    pub octaves: i32,
    pub falloff: f64,
    pub elevation_scale: f64,
    pub elevation_lac: f64,
    pub temperature_scale: f64,
    pub temperature_lac: f64,
    pub moisture_scale: f64,
    pub moisture_lac: f64,
    pub river_scale: f64,
    pub river_lac: f64,
    pub cave_scale: f64,
    pub cave_lac: f64,
    pub cave_grad_base: f64,
    pub cave_grad_scale: f64,
    pub biomes: HashMap<String, BiomeConfig>,
    pub ores: HashMap<String, OreConfig>,
}

impl WorldGenConfig {
    pub fn load(path: &str) -> WorldGenConfig {
        let data = fs::read_to_string(path).expect("World generator config not found");
        toml::from_str::<WorldGenConfig>(data.as_str())
            .expect("Failed to parse world generator config")
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub motd: String,
    pub slots: i32,
    pub game_mode: GameMode,
    pub difficulty: u8,
    pub net_endpoint: String,
    pub net_compression: usize,
    pub generator_threads: u32,
    pub view_dist: i32,
    pub seed: Option<u32>,
}

impl ServerConfig {
    pub fn load(path: &str) -> ServerConfig {
        let data = fs::read_to_string(path).expect("Server config not found");
        toml::from_str::<ServerConfig>(data.as_str()).expect("Failed to parse server config")
    }
}
