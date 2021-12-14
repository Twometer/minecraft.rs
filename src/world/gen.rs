use std::sync::Arc;

use crate::{block_state, world::World};

pub struct WorldGenerator {
    world: Arc<World>,
}

impl WorldGenerator {
    pub fn new(world: Arc<World>) -> WorldGenerator {
        WorldGenerator { world }
    }

    pub fn generate(&self) {
        for y in 0..64 {
            for z in -176..=176 {
                for x in -176..=176 {
                    if y == 63 {
                        self.world.set_block(x, y, z, block_state!(2, 0));
                    } else {
                        self.world.set_block(x, y, z, block_state!(1, 0));
                    }
                }
            }
        }
    }
}
