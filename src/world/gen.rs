use crate::{block_state, world::World};

pub struct WorldGenerator<'a> {
    world: &'a World,
}

impl<'a> WorldGenerator<'a> {
    pub fn new(world: &World) -> WorldGenerator {
        WorldGenerator { world }
    }

    pub fn generate(&self) {
        for y in 0..256 {
            for z in 0..256 {
                for x in 0..256 {
                    self.world.set_block(x, y, z, block_state!(1, 0));
                }
            }
        }
    }
}
