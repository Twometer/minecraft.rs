use std::{ops::Add, sync::Arc, time::Duration};

use dashmap::DashSet;
use futures::{SinkExt, StreamExt};
use log::{error, info, trace};
use serde_json::json;
use tokio::{
    net::TcpStream,
    select,
    sync::mpsc,
    time::{self, Instant},
};
use tokio_util::codec::Framed;

use crate::{
    config::ServerConfig,
    mc::{codec::MinecraftCodec, proto::Packet, proto::PlayState},
    utils::broadcast_chat,
    world::{sched::GenerationScheduler, Chunk, ChunkPos, MutexChunkRef, World},
};

pub struct ClientHandler {
    in_stream: Framed<TcpStream, MinecraftCodec>,
    out_stream: mpsc::Receiver<Packet>,
    broadcast: mpsc::Sender<Packet>,
    world: Arc<World>,
    world_gen: Arc<GenerationScheduler>,
    server_config: Arc<ServerConfig>,
    entity_id: i32,
    username: String,
    known_chunks: DashSet<ChunkPos>,
    current_chunk_pos: ChunkPos,
}

impl ClientHandler {
    pub fn new(
        in_stream: Framed<TcpStream, MinecraftCodec>,
        out_stream: mpsc::Receiver<Packet>,
        broadcast: mpsc::Sender<Packet>,
        world: Arc<World>,
        world_gen: Arc<GenerationScheduler>,
        server_config: Arc<ServerConfig>,
    ) -> ClientHandler {
        ClientHandler {
            in_stream,
            out_stream,
            broadcast,
            world,
            world_gen,
            server_config,
            entity_id: 0,
            username: String::new(),
            known_chunks: DashSet::new(),
            current_chunk_pos: ChunkPos::new(0, 0),
        }
    }

    pub async fn handle_loop(&mut self) {
        let mut keep_alive_interval = time::interval_at(
            Instant::now().add(Duration::from_secs(5)),
            Duration::from_secs(10),
        );

        loop {
            select! {
                packet_in = self.in_stream.next() => {
                    if packet_in.is_none() {
                        break;
                    }

                    match packet_in.unwrap() {
                        Ok(packet) => {
                            self.handle_packet(packet)
                                .await
                                .expect("Packet handler failed");
                        }
                        Err(err) => {
                            error!("Client receive failed: {}", err);
                            break;
                        }
                    }

                },
                packet_out = self.out_stream.recv() => {
                    if packet_out.is_none() {
                        break;
                    }

                    self.in_stream.send(packet_out.unwrap()).await.expect("Client send failed");
                }
                _ = keep_alive_interval.tick() => {
                    self.in_stream
                        .send(Packet::S00KeepAlive { timestamp: 69 })
                        .await
                        .expect("Client keep-alive failed");
                }
            }
        }

        self.in_stream.close().await.unwrap();
        self.out_stream.close();
    }

    async fn handle_packet(&mut self, packet: Packet) -> std::io::Result<()> {
        trace!("Received {:?}", packet);

        match packet {
            Packet::C00Handshake {
                protocol_version,
                next_state,
                ..
            } => {
                if protocol_version != 47 {
                    panic!("Unsupported protocol version");
                }

                self.in_stream.codec_mut().change_state(next_state);
            }

            Packet::C00StatusRequest => {
                let status = json!({
                    "version": {
                        "name": "1.8.0",
                        "protocol": 47
                    },
                    "players":{
                        "max": self.server_config.slots,
                        "online": 0,
                        "sample": []
                    },
                    "description": {
                        "text": self.server_config.motd
                    }
                });
                self.in_stream
                    .send(Packet::S00StatusResponse {
                        status: status.to_string(),
                    })
                    .await?;
            }
            Packet::C01StatusPing { timestamp } => {
                self.in_stream
                    .send(Packet::S01StatusPong { timestamp })
                    .await?;
            }

            Packet::C00LoginStart { username } => {
                self.username = username;

                // Enable compression
                self.in_stream
                    .send(Packet::S03LoginCompression {
                        threshold: self.server_config.net_compression as i32,
                    })
                    .await?;
                self.in_stream
                    .codec_mut()
                    .change_compression_threshold(self.server_config.net_compression);

                // Enter play state
                self.in_stream
                    .send(Packet::S02LoginSuccess {
                        uuid: "3b9f9997-d547-4f70-a37c-8fffbe706002".to_string(),
                        username: self.username.clone(),
                    })
                    .await?;
                self.in_stream.codec_mut().change_state(PlayState::Play);

                // Complete login sequence
                self.in_stream
                    .send(Packet::S01JoinGame {
                        entity_id: self.entity_id,
                        gamemode: self.server_config.gamemode,
                        dimension: 0,
                        difficulty: self.server_config.difficulty,
                        player_list_size: 4,
                        world_type: "default".to_string(),
                        reduced_debug_info: false,
                    })
                    .await?;

                // Transmit world
                self.send_world(0, 0, self.server_config.view_dist).await?;

                // Spawn player into world
                self.in_stream
                    .send(Packet::S08SetPlayerPosition {
                        x: 0.0,
                        y: 64.0,
                        z: 0.0,
                        yaw: 0.0,
                        pitch: 0.0,
                        flags: 0,
                    })
                    .await?;

                // Announce login
                info!(
                    "{} logged in with entity id {}",
                    self.username, self.entity_id
                );
                broadcast_chat(
                    &mut self.broadcast,
                    format!("§e{} joined the game", self.username),
                )
                .await;
            }
            Packet::C01ChatMessage { message } => {
                let prepared_message = format!("§b{}§r: {}", self.username, message);
                info!("Chat message: {}", prepared_message);
                broadcast_chat(&mut self.broadcast, prepared_message).await;
            }
            Packet::C07PlayerDigging { location, .. } => {
                // TODO Sanitize position
                self.world.set_block(location.x, location.y, location.z, 0);
            }
            Packet::C04PlayerPos { x, z, .. } => {
                self.update_chunks(ChunkPos::from_block_pos(x as i32, z as i32))
                    .await?;
            }
            Packet::C06PlayerPosRot { x, z, .. } => {
                self.update_chunks(ChunkPos::from_block_pos(x as i32, z as i32))
                    .await?;
            }
            _ => {
                trace!("Received unhandled packet: {:?}", packet);
            }
        }

        Ok(())
    }

    async fn update_chunks(&mut self, center: ChunkPos) -> std::io::Result<()> {
        if center != self.current_chunk_pos {
            self.current_chunk_pos = center;

            let r = self.server_config.view_dist;
            self.world_gen.request_region(center.x, center.z, r);
            self.world_gen.await_region(center.x, center.z, r).await;
            self.send_world(center.x, center.z, r).await?;

            let min_x = center.x - r;
            let min_z = center.z - r;
            let max_x = center.x + r;
            let max_z = center.z + r;

            let removed = self
                .known_chunks
                .iter()
                .filter(|k| k.x < min_x || k.z < min_z || k.x > max_x || k.z > max_z)
                .map(|k| *k)
                .collect::<Vec<ChunkPos>>();

            for r in removed {
                self.in_stream
                    .send(Packet::S21ChunkData { x: r.x, z: r.z })
                    .await?;
                self.known_chunks.remove(&r);
            }
        }

        Ok(())
    }

    async fn send_world(&mut self, center_x: i32, center_z: i32, r: i32) -> std::io::Result<()> {
        let mut chunk_refs = Vec::<MutexChunkRef>::new();

        // Collect chunks to be sent
        for z in -r..=r {
            for x in -r..=r {
                let chunk_pos = ChunkPos::new(center_x + x, center_z + z);
                let chunk_opt = self.world.get_chunk(chunk_pos);
                if chunk_opt.is_some() && !self.known_chunks.contains(&chunk_pos) {
                    chunk_refs.push(chunk_opt.unwrap());
                    self.known_chunks.insert(chunk_pos);
                }
            }
        }

        // Split into packets
        let chunks_per_packet = 10;

        for subslice in chunk_refs.chunks(chunks_per_packet) {
            let mut transmit_chunks = Vec::<Chunk>::new();
            for chunk_ref in subslice {
                transmit_chunks.push(chunk_ref.lock().unwrap().clone())
            }

            self.in_stream
                .send(Packet::S26MapChunkBulk {
                    skylight: true,
                    chunks: transmit_chunks,
                })
                .await?;
        }

        Ok(())
    }
}
