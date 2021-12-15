use std::sync::Arc;

use futures::executor::block_on;
use noise::{NoiseFn, SuperSimplex};

use crate::{block_state, world::Chunk, world::World};

pub struct WorldGenerator {
    world: Arc<World>,
    noise: SuperSimplex,
}

impl WorldGenerator {
    pub fn new(world: Arc<World>) -> WorldGenerator {
        WorldGenerator {
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

                let noise_val = self.noise_sample(world_x, world_z, 0.05);
                let h = 48 + ((noise_val + 1.0) * 0.5 * 16.0) as i32;

                for y in 0..=h {
                    chunk.set_block(x, y, z, self.determine_block(world_x, y, world_z, h));
                }
            }
        }

        self.world.insert_chunk(chunk);
    }

    fn determine_block(&self, x: i32, y: i32, z: i32, h: i32) -> u16 {
        if y == h {
            block_state!(2, 0)
        } else if y >= h - 3 {
            block_state!(3, 0)
        } else {
            block_state!(1, 0)
        }
    }

    fn noise_sample(&self, x: i32, z: i32, scale: f64) -> f64 {
        self.noise.get([x as f64 * scale, z as f64 * scale])
    }
}
