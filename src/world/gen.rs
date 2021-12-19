use std::{panic, sync::Arc};

use log::debug;
use noise::{NoiseFn, Seedable, SuperSimplex};
use rand::Rng;

use crate::{
    block_state,
    config::{BiomeConfig, BiomeLayer, WorldGenConfig},
    world::Chunk,
    world::World,
};

use super::{math::dist, ChunkPos};

pub struct WorldGenerator {
    config: WorldGenConfig,
    world: Arc<World>,
    noise: SuperSimplex,
}

impl WorldGenerator {
    pub fn new(seed: u32, config: WorldGenConfig, world: Arc<World>) -> WorldGenerator {
        debug!("Using seed {} for world generation", seed);

        WorldGenerator {
            config,
            world,
            noise: SuperSimplex::new().set_seed(seed),
        }
    }

    pub fn generate_chunk(&self, chunk_x: i32, chunk_z: i32) {
        let pos = ChunkPos::new(chunk_x, chunk_z);

        match self.world.get_chunk(pos) {
            Some(chunk) => {
                let mut chunk = chunk.lock().unwrap();
                self.generate_into_chunk(&mut *chunk);
            }
            None => {
                let mut chunk = Chunk::new(chunk_x, chunk_z);
                self.generate_into_chunk(&mut chunk);
                self.world.insert_chunk(chunk);
            }
        }
    }

    fn generate_into_chunk(&self, chunk: &mut Chunk) {
        let base_x = chunk.x << 4;
        let base_z = chunk.z << 4;

        for x in 0..16 {
            for z in 0..16 {
                let world_x = base_x + x;
                let world_z = base_z + z;

                self.generate_column(chunk, x, z, world_x, world_z)
            }
        }
    }

    fn generate_column(&self, chunk: &mut Chunk, x: i32, z: i32, world_x: i32, world_z: i32) {
        let (elevation, biome) = self.sample_biome(world_x, world_z);
        let interp_scale = self.multi_sample_biome_scale(world_x, world_z, 3);

        let noise_val = elevation * interp_scale;
        let terrain_height = (noise_val * 16.0) as i32 + 64;
        let generate_height = if biome.sea_level { 64 } else { terrain_height };
        let mut top_layer_height = 0;
        let mut top_layer_state = 0;

        // Convert heightmap to blocks
        for y in 0..=generate_height {
            let block_state =
                self.determine_block(world_x, y, world_z, terrain_height, generate_height, biome);

            if block_state != 0 {
                top_layer_height = y + 1;
                top_layer_state = block_state;
                chunk.set_block(x, y, z, block_state);
            }
        }

        if top_layer_state == block_state!(3, 0) {
            chunk.set_block(x, top_layer_height, z, block_state!(2, 0));
        }

        // Apply surface layer
        if biome.surface_layer.is_some() {
            chunk.set_block(
                x,
                top_layer_height,
                z,
                block_state!(biome.surface_layer.unwrap(), 0),
            );
        }

        // Generate features
        for (feature, prob) in &biome.features {
            if self.should_generate(*prob) {
                self.generate_feature(feature, chunk, x, top_layer_height, z);
            }
        }

        // Set biome
        chunk.set_biome(x, z, biome.id);
    }

    fn generate_feature(&self, feature: &str, chunk: &mut Chunk, x: i32, top_y: i32, z: i32) {
        match feature {
            "grass" => {
                chunk.set_block_if_air(x, top_y, z, block_state!(31, 1));
            }
            "fern" => {
                chunk.set_block_if_air(x, top_y, z, block_state!(31, 2));
            }
            "bushes" => {
                chunk.set_block_if_air(x, top_y, z, block_state!(18, 3));
            }
            "dead_bushes" => {
                chunk.set_block_if_air(x, top_y, z, block_state!(32, 0));
            }
            "flowers" => {
                chunk.set_block_if_air(x, top_y, z, block_state!(38, 0));
            }
            "mushrooms" => {
                chunk.set_block_if_air(x, top_y, z, block_state!(39, 0));
            }
            "puddles" => {
                chunk.set_block(x, top_y - 1, z, block_state!(9, 0));
            }
            "lilypads" => {
                chunk.set_block_if_air(x, top_y, z, block_state!(111, 0));
            }
            "boulders" => {
                chunk.set_block(x, top_y - 1, z, block_state!(1, 5));
            }
            "cacti" => {
                for i in 0..3 {
                    chunk.set_block(x, top_y + i, z, block_state!(81, 0));
                }
            }
            "icicles" => {
                for i in 0..3 {
                    chunk.set_block(x, top_y + i, z, block_state!(174, 0));
                }
            }
            "warm_tree" => {
                Self::generate_tree(
                    chunk,
                    x,
                    top_y,
                    z,
                    6,
                    block_state!(17, 0),
                    block_state!(18, 0),
                );
            }
            "cold_tree" => {
                Self::generate_tree(
                    chunk,
                    x,
                    top_y,
                    z,
                    6,
                    block_state!(17, 1),
                    block_state!(18, 1),
                );
            }
            "jungle_tree" => {
                Self::generate_tree(
                    chunk,
                    x,
                    top_y,
                    z,
                    9,
                    block_state!(17, 3),
                    block_state!(18, 3),
                );
            }
            _ => panic!("Unknown feature {}", feature),
        }
    }

    fn generate_tree(
        chunk: &mut Chunk,
        x: i32,
        y: i32,
        z: i32,
        height: i32,
        trunk_block: u16,
        leaves_block: u16,
    ) {
        if !Self::check_surroundings(chunk, x, y, z, 2, trunk_block) {
            for i in 0..height {
                if i > height - 5 {
                    let r = (height - i).min(2);
                    for xo in -r..=r {
                        for zo in -r..=r {
                            if i < height - 2
                                || xo * xo + zo * zo <= r * r + rand::thread_rng().gen_range(0..1)
                            {
                                chunk.set_block(x + xo, y + i, z + zo, leaves_block)
                            }
                        }
                    }
                }

                if i < height - 2 {
                    chunk.set_block(x, y + i, z, trunk_block);
                }
            }
        }
    }

    fn check_surroundings(chunk: &Chunk, x: i32, y: i32, z: i32, r: i32, state: u16) -> bool {
        for xo in -r..=r {
            for yo in -r..=r {
                for zo in -r..=r {
                    if chunk.get_block(x + xo, y + yo, z + zo) == state {
                        return true;
                    }
                }
            }
        }
        return false;
    }

    fn should_generate(&self, prob: f64) -> bool {
        rand::thread_rng().gen_bool(prob)
    }

    fn determine_block_underground(&self, x: i32, y: i32, z: i32) -> u16 {
        for (_, ore) in &self.config.ores {
            let offset = ore.id as f64 * 1000.0;
            let noise = self.noise.get([
                x as f64 * ore.scale,
                y as f64 * ore.scale + offset,
                z as f64 * ore.scale,
            ]);

            let diff = (ore.center - (y as f64)).abs();
            if diff > ore.spread {
                continue;
            }

            let offset = diff / ore.spread;
            let threshold = ore.threshold + (offset * 0.055);
            if noise > threshold {
                return block_state!(ore.id, 0);
            }
        }

        block_state!(1, 0)
    }

    fn is_cave(&self, world_x: i32, y: i32, world_z: i32, h: i32) -> bool {
        let n1 = self.sample_noise_fractal_3d(
            world_x,
            y,
            world_z,
            self.config.cave_scale,
            self.config.cave_lac,
        );
        let n2 = self.sample_noise_fractal_3d(
            world_x + 1024,
            y,
            world_z + 1024,
            self.config.cave_scale,
            self.config.cave_lac,
        );

        let height_gradient = (y as f64) / (h as f64); // [0..1]
        let cave_th = self.config.cave_grad_base + height_gradient * self.config.cave_grad_scale;
        n1 > cave_th && n2 > cave_th
    }

    fn determine_block(
        &self,
        x: i32,
        y: i32,
        z: i32,
        th: i32,
        gh: i32,
        biome: &BiomeConfig,
    ) -> u16 {
        let is_cave = y <= th && self.is_cave(x, y, z, th);
        let cave_block = if is_cave && y <= 8 {
            11
        } else if is_cave {
            0
        } else {
            1
        };

        if y == gh {
            if is_cave && !biome.sea_level {
                block_state!(cave_block, 0)
            } else {
                block_state!(biome.blocks[0], 0)
            }
        } else if y >= th {
            if is_cave && !biome.sea_level {
                block_state!(cave_block, 0)
            } else {
                block_state!(biome.blocks[1], 0)
            }
        } else if y >= th - 3 {
            if is_cave && !biome.sea_level {
                block_state!(cave_block, 0)
            } else {
                block_state!(biome.blocks[2], 0)
            }
        } else if y > 3 {
            if is_cave {
                block_state!(cave_block, 0)
            } else {
                self.determine_block_underground(x, y, z)
            }
        } else if y > 0 {
            block_state!(
                if rand::thread_rng().gen_bool(0.5) {
                    7
                } else {
                    cave_block
                },
                0
            )
        } else {
            block_state!(7, 0)
        }
    }

    fn multi_sample_biome_scale(&self, x: i32, z: i32, r: i32) -> f64 {
        let mut total = 0.0;
        let mut denom = 0.0;
        for x_offset in -r..=r {
            for z_offset in -r..=r {
                total += self.sample_biome(x + x_offset, z + z_offset).1.scale;
                denom += 1.0;
            }
        }
        total / denom
    }

    fn sample_biome(&self, x: i32, z: i32) -> (f64, &BiomeConfig) {
        let elevation =
            self.sample_noise_fractal(x, z, self.config.elevation_scale, self.config.elevation_lac);
        let temperature = self.sample_noise_fractal(
            -x,
            z,
            self.config.temperature_scale,
            self.config.temperature_lac,
        );
        let moisture =
            self.sample_noise_fractal(x, -z, self.config.moisture_scale, self.config.moisture_lac);
        let river = (self
            .sample_noise_fractal(-x, -z, self.config.river_scale, self.config.river_lac)
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
    ) -> &BiomeConfig {
        let layer = if elevation >= self.config.ocean_level - 0.025 && river < 0.015 {
            BiomeLayer::River
        } else if (elevation - self.config.ocean_level).abs() < 0.025 {
            return &self.config.biomes["beach"];
        } else if elevation < self.config.ocean_level {
            BiomeLayer::Sea
        } else {
            BiomeLayer::Land
        };

        let mut best_biome_name = "forest";
        let mut best_biome_dist = f64::MAX;

        for (name, biome) in &self.config.biomes {
            if biome.layer != layer {
                continue;
            }

            let de = dist(elevation, biome.elevation);
            let dt = dist(temperature, biome.temperature);
            let dm = dist(moisture, biome.moisture);
            let d = de * de + dt * dt + dm * dm;
            if d < best_biome_dist {
                best_biome_dist = d;
                best_biome_name = name;
            }
        }

        return &self.config.biomes[best_biome_name];
    }

    fn sample_noise_fractal(&self, x: i32, z: i32, mut scale: f64, lac: f64) -> f64 {
        let mut result = 0.0;
        let mut denom = 0.0;
        scale *= self.config.master_scale;

        let mut amplitude = 1.0;
        for _ in 0..self.config.octaves {
            result += amplitude * self.noise.get([x as f64 * scale, z as f64 * scale]);
            denom += amplitude;

            scale *= lac;
            amplitude *= self.config.falloff;
        }

        result / denom
    }

    fn sample_noise_fractal_3d(&self, x: i32, y: i32, z: i32, mut scale: f64, lac: f64) -> f64 {
        let mut result = 0.0;
        let mut denom = 0.0;

        let mut amplitude = 1.0;
        for _ in 0..3 {
            result += amplitude
                * self
                    .noise
                    .get([x as f64 * scale, y as f64 * scale, z as f64 * scale]);
            denom += amplitude;

            scale *= lac;
            amplitude *= self.config.falloff;
        }

        result / denom
    }
}
