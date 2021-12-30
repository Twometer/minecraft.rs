use std::{collections::HashSet, sync::Arc};

use dashmap::DashSet;
use flume::{Receiver, Sender};
use tokio::sync::broadcast;

use super::{gen::WorldGenerator, ChunkPos, World};

pub struct GenerationScheduler {
    world: Arc<World>,
    generator: Arc<WorldGenerator>,
    pending: Arc<DashSet<ChunkPos>>,
    request_tx: Sender<ChunkPos>,
    request_rx: Receiver<ChunkPos>,
    completion_bc: broadcast::Sender<ChunkPos>,
}

impl GenerationScheduler {
    pub fn new(
        world: Arc<World>,
        generator: Arc<WorldGenerator>,
        num_threads: u32,
    ) -> GenerationScheduler {
        let (tx, rx) = flume::unbounded();
        let (completion_bc, _) = broadcast::channel::<ChunkPos>(128);

        let scheduler = GenerationScheduler {
            world,
            generator,
            pending: Arc::new(DashSet::new()),
            request_tx: tx,
            request_rx: rx,
            completion_bc,
        };
        scheduler.start(num_threads);
        scheduler
    }

    fn start(&self, num_threads: u32) {
        for _ in 0..num_threads {
            let generator = self.generator.clone();
            let pending = self.pending.clone();
            let rx = self.request_rx.clone();
            let bc = self.completion_bc.clone();

            std::thread::spawn(move || loop {
                let chunk = rx.recv().expect("failed to recv from chunk queue");
                generator.generate_chunk(chunk.x, chunk.z);
                pending.remove(&chunk);
                let _ = bc.send(chunk);
            });
        }
    }

    pub fn request_region(&self, center_x: i32, center_z: i32, r: i32) {
        for x in -r..=r {
            for z in -r..=r {
                self.request_chunk(center_x + x, center_z + z);
            }
        }
    }

    pub async fn await_region(&self, center_x: i32, center_z: i32, r: i32) {
        let mut receiver = self.completion_bc.subscribe();
        let mut remaining_chunks = HashSet::<ChunkPos>::new();
        for x in -r..=r {
            for z in -r..=r {
                let pos = ChunkPos::new(center_x + x, center_z + z);
                if !self.world.has_chunk(pos) {
                    remaining_chunks.insert(pos);
                }
            }
        }

        while !remaining_chunks.is_empty() {
            let generated_chunk = receiver
                .recv()
                .await
                .expect("Failed to read chunk completion");
            remaining_chunks.remove(&generated_chunk);
        }
    }

    fn request_chunk(&self, x: i32, z: i32) {
        let pos = ChunkPos::new(x, z);
        if !self.pending.contains(&pos) && !self.world.has_chunk(pos) {
            self.pending.insert(pos);
            self.request_tx
                .send(pos)
                .expect("failed to send to chunk queue");
        }
    }
}
