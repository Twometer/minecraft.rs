use std::sync::Arc;

use noise::{NoiseFn, SuperSimplex};

use crate::{
    block_state,
    config::{BiomeConfig, BiomeLayer, WorldGenConfig},
    world::Chunk,
    world::World,
};

pub struct WorldGenerator {
    config: WorldGenConfig,
    world: Arc<World>,
    noise: SuperSimplex,
}

impl WorldGenerator {
    pub fn new(config: WorldGenConfig, world: Arc<World>) -> WorldGenerator {
        WorldGenerator {
            config,
            world,
            noise: SuperSimplex::new(),
        }
    }

    pub fn generate(&self) {
        for x in -10..=10 {
            for z in -10..=10 {
                self.generate_chunk(x, z)
            }
        }
    }

    fn generate_chunk(&self, chunk_x: i32, chunk_z: i32) {
        let mut chunk = Chunk::new(chunk_x, chunk_z);
        let base_x = chunk_x << 4;
        let base_z = chunk_z << 4;

        for x in 0..16 {
            for z in 0..16 {
                let world_x = base_x + x;
                let world_z = base_z + z;

                let (elevation, biome) = self.sample_biome(world_x, world_z);
                let noise_val = elevation * biome.scale;
                let mut h = 64;
                if !biome.sea_level {
                    h += (noise_val * 16.0) as i32;
                }

                for y in 0..=h {
                    chunk.set_block(
                        x,
                        y,
                        z,
                        self.determine_block(world_x, y, world_z, h, &biome),
                    );
                }
                chunk.set_biome(x, z, biome.id);
            }
        }

        self.world.insert_chunk(chunk);
    }

    fn determine_block(&self, x: i32, y: i32, z: i32, h: i32, biome: &BiomeConfig) -> u16 {
        if y >= h {
            block_state!(biome.top_block, 0)
        } else if y >= h - 3 {
            block_state!(3, 0)
        } else {
            block_state!(1, 0)
        }
    }

    fn sample_biome(&self, x: i32, z: i32) -> (f64, BiomeConfig) {
        let elevation =
            self.sample_noise_fractal(x, z, self.config.elevation_scale, self.config.elevation_lac);
        let temperature = self.sample_noise_fractal(
            x,
            z,
            self.config.temperature_scale,
            self.config.temperature_lac,
        );
        let moisture =
            self.sample_noise_fractal(x, z, self.config.moisture_scale, self.config.moisture_lac);
        let river = (self
            .sample_noise_fractal(x, z, self.config.river_scale, self.config.river_lac)
            .abs())
            * (elevation + 1.0)
            * 0.5;

        (
            elevation,
            self.find_biome(temperature, moisture, elevation, river),
        )
    }

    fn find_biome(
        &self,
        temperature: f64,
        moisture: f64,
        elevation: f64,
        river: f64,
    ) -> BiomeConfig {
        if (elevation - self.config.ocean_level).abs() < 0.025 {
            return self.config.biomes["beach"];
        }

        let layer = if elevation >= self.config.ocean_level && river < 0.015 {
            BiomeLayer::River
        } else if elevation < self.config.ocean_level {
            BiomeLayer::Sea
        } else {
            BiomeLayer::Land
        };

        let mut best_biome_name = "forest".to_string();
        let mut best_biome_dist = f64::MAX;

        for (name, biome) in &self.config.biomes {
            if biome.layer != layer {
                continue;
            }

            let de = Self::dist(elevation, biome.elevation);
            let dt = Self::dist(temperature, biome.temperature);
            let dm = Self::dist(moisture, biome.moisture);
            let d = de * de + dt * dt + dm * dm;
            if d < best_biome_dist {
                best_biome_dist = d;
                best_biome_name = name.to_string();
            }
        }

        return self.config.biomes[best_biome_name.as_str()];
    }

    fn sample_noise_fractal(&self, x: i32, z: i32, mut scale: f64, lac: f64) -> f64 {
        let mut result = 0.0;
        let mut denom = 0.0;
        scale *= self.config.master_scale;

        let mut amplitude = 1.0;
        for i in 0..self.config.octaves {
            result += amplitude * self.noise.get([x as f64 * scale, z as f64 * scale]);
            denom += amplitude;

            scale *= lac;
            amplitude *= self.config.falloff;
        }

        result / denom
    }

    fn dist(a: f64, b: Option<f64>) -> f64 {
        if b.is_none() {
            0.0
        } else {
            b.unwrap() - a
        }
    }
}
