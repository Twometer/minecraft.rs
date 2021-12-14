use std::{ops::Add, sync::Arc, time::Duration};

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
    mc::{codec::MinecraftCodec, proto::Packet, proto::PlayState},
    utils::broadcast_chat,
    world::{Chunk, ChunkPos, MutexChunkRef, World},
};

pub struct ClientHandler {
    in_stream: Framed<TcpStream, MinecraftCodec>,
    out_stream: mpsc::Receiver<Packet>,
    broadcast: mpsc::Sender<Packet>,
    world: Arc<World>,
    entity_id: i32,
    username: String,
}

impl ClientHandler {
    pub fn new(
        in_stream: Framed<TcpStream, MinecraftCodec>,
        out_stream: mpsc::Receiver<Packet>,
        broadcast: mpsc::Sender<Packet>,
        world: Arc<World>,
    ) -> ClientHandler {
        ClientHandler {
            in_stream,
            out_stream,
            broadcast,
            world,
            entity_id: 0,
            username: String::new(),
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
                        "max": 20,
                        "online": 0,
                        "sample": []
                    },
                    "description": {
                        "text": "Hello from §6minecraft.rs §rwith §aT§bo§ck§di§eo"
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
                    .send(Packet::S03LoginCompression { threshold: 256 })
                    .await?;
                self.in_stream.codec_mut().change_compression_threshold(256);

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
                        gamemode: 1,
                        dimension: 0,
                        difficulty: 0,
                        player_list_size: 4,
                        world_type: "default".to_string(),
                        reduced_debug_info: false,
                    })
                    .await?;

                // Transmit world
                self.send_world(-10, -10, 10, 10).await?;

                // Spawn playef into world
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
            _ => {
                trace!("Received unhandled packet: {:?}", packet);
            }
        }

        Ok(())
    }

    async fn send_world(&mut self, x0: i32, z0: i32, x1: i32, z1: i32) -> std::io::Result<()> {
        let mut chunk_refs = Vec::<MutexChunkRef>::new();

        // Collect chunks to be sent
        for z in z0..=z1 {
            for x in x0..=x1 {
                let chunk_opt = self.world.get_chunk(ChunkPos::new(x, z));
                if chunk_opt.is_some() {
                    chunk_refs.push(chunk_opt.unwrap());
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
