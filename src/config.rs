use std::{collections::HashMap, fs};

use serde_derive::Deserialize;

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
}

#[derive(Debug, Deserialize)]
pub struct WorldGenConfig {
    pub master_scale: f64,
    pub ocean_level: f64,
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
    pub biomes: HashMap<String, BiomeConfig>,
}

impl WorldGenConfig {
    pub fn load(path: &str) -> WorldGenConfig {
        let data = fs::read_to_string(path).expect("World generator config not found");
        toml::from_str::<WorldGenConfig>(data.as_str())
            .expect("Failed to parse world generator config")
    }
}
