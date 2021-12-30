use std::{ops::Add, sync::Arc, time::Duration};

use dashmap::DashSet;
use futures::{SinkExt, StreamExt};
use indoc::indoc;
use log::{debug, error, info, trace};
use serde_json::json;
use tokio::{
    io,
    net::TcpStream,
    select,
    sync::mpsc,
    time::{self, Instant},
};
use tokio_util::codec::Framed;

use crate::{
    block_id, block_meta, block_state, chat_packet,
    command::Command,
    mc::{
        codec::MinecraftCodec,
        proto::{
            AbilityFlags, DiggingStatus, EntityMetaData, EntityMetaEntry, GameStateReason, Packet,
        },
        proto::{PlayState, PlayerListItemAction},
    },
    model::{GameMode, ItemStack, Player},
    server::ServerHandler,
    world::{BlockFace, BlockPos, Chunk, ChunkPos, MutexChunkRef},
};

pub struct ClientHandler {
    msg_stream: Framed<TcpStream, MinecraftCodec>,
    unicast_rx: mpsc::Receiver<Packet>,
    server: Arc<ServerHandler>,
    player: Player,
    known_chunks: DashSet<ChunkPos>,
    current_chunk_pos: ChunkPos,
}

impl ClientHandler {
    pub fn new(
        id: i32,
        msg_stream: Framed<TcpStream, MinecraftCodec>,
        unicast_rx: mpsc::Receiver<Packet>,
        server: Arc<ServerHandler>,
    ) -> ClientHandler {
        let game_mode = server.config.game_mode;
        ClientHandler {
            msg_stream,
            unicast_rx,
            server,
            player: Player::new(id, game_mode),
            known_chunks: DashSet::new(),
            current_chunk_pos: ChunkPos::new(0, 0),
        }
    }

    pub async fn loop_until_disconnect(&mut self) {
        let mut keep_alive_interval = time::interval_at(
            Instant::now().add(Duration::from_secs(5)),
            Duration::from_secs(10),
        );

        loop {
            select! {
                packet_in = self.msg_stream.next() => {
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
                packet_out = self.unicast_rx.recv() => {
                    if packet_out.is_none() {
                        break;
                    }

                    self.msg_stream.send(packet_out.unwrap()).await.expect("Client send failed");
                }
                _ = keep_alive_interval.tick() => {
                    self.msg_stream
                        .send(Packet::S00KeepAlive { timestamp: 69 })
                        .await
                        .expect("Client keep-alive failed");
                }
            }
        }

        self.msg_stream.close().await.unwrap();
        self.unicast_rx.close();
        self.server.remove_client(self.player.eid);
        if self.player.is_logged_in() {
            self.server.change_num_players(-1);
        }
    }

    async fn handle_packet(&mut self, packet: Packet) -> io::Result<()> {
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

                self.msg_stream.codec_mut().set_state(next_state);
            }

            Packet::C00StatusRequest => {
                let status = json!({
                    "version": {
                        "name": "1.8.0",
                        "protocol": 47
                    },
                    "players":{
                        "max": self.server.config.slots,
                        "online": self.server.num_players(),
                        "sample": []
                    },
                    "description": {
                        "text": self.server.config.motd
                    }
                });
                self.send_packet(Packet::S00StatusResponse {
                    status: status.to_string(),
                })
                .await?;
            }
            Packet::C01StatusPing { timestamp } => {
                self.send_packet(Packet::S01StatusPong { timestamp })
                    .await?;
            }

            Packet::C00LoginStart { username } => {
                self.player.username = username;
                self.server.change_num_players(1);

                // Enable compression
                self.send_packet(Packet::S03LoginCompression {
                    threshold: self.server.config.net_compression as i32,
                })
                .await?;
                self.msg_stream
                    .codec_mut()
                    .set_compression_threshold(self.server.config.net_compression);

                // Enter play state
                self.send_packet(Packet::S02LoginSuccess {
                    uuid: self.player.uuid.to_string(),
                    username: self.player.username.clone(),
                })
                .await?;
                self.msg_stream.codec_mut().set_state(PlayState::Play);

                // Complete login sequence
                self.send_packet(Packet::S01JoinGame {
                    entity_id: self.player.eid,
                    game_mode: self.player.game_mode,
                    dimension: 0,
                    difficulty: self.server.config.difficulty,
                    player_list_size: 4,
                    world_type: "default".to_string(),
                    reduced_debug_info: false,
                })
                .await?;

                // Send world chunks
                self.send_chunks(0, 0, self.server.config.view_dist).await?;

                // Spawn player into world
                self.send_packet(Packet::S08SetPlayerPosition {
                    x: 0.0,
                    y: 69.0,
                    z: 0.0,
                    yaw: 0.0,
                    pitch: 0.0,
                    flags: 0,
                })
                .await?;

                // Announce player join
                info!(
                    "{} logged in with entity id {}",
                    self.player.username, self.player.eid
                );
                self.server
                    .send_broadcast(chat_packet!(
                        1,
                        format!("§e{} joined the game", self.player.username)
                    ))
                    .await?;
                self.server
                    .send_broadcast(Packet::S38PlayerListItem {
                        uuid: self.player.uuid,
                        action: PlayerListItemAction::AddPlayer {
                            name: self.player.username.clone(),
                            game_mode: self.player.game_mode,
                            display_name: None,
                            ping: 0,
                        },
                    })
                    .await?;
            }
            Packet::C01ChatMessage { message } => {
                let message = message.as_str();
                if message.starts_with("/") {
                    self.handle_command(message).await?;
                } else {
                    info!("Chat message: <{}> {}", self.player.username, message);

                    let formatted_message = format!("§b{}§r: {}", self.player.username, message);
                    self.server
                        .send_broadcast(chat_packet!(0, formatted_message))
                        .await?;
                }
            }
            Packet::C04PlayerPos { x, y, z, .. } => {
                self.player.position.x = x;
                self.player.position.y = y;
                self.player.position.z = z;
                self.update_chunks(ChunkPos::from_block_pos(x as i32, z as i32))
                    .await?;
            }
            Packet::C05PlayerRot { yaw, pitch, .. } => {
                self.player.rotation.x = yaw;
                self.player.rotation.y = pitch;
            }
            Packet::C06PlayerPosRot {
                x,
                y,
                z,
                yaw,
                pitch,
                ..
            } => {
                self.player.position.x = x;
                self.player.position.y = y;
                self.player.position.z = z;
                self.player.rotation.x = yaw;
                self.player.rotation.y = pitch;
                self.update_chunks(ChunkPos::from_block_pos(x as i32, z as i32))
                    .await?;
            }
            Packet::C07PlayerDigging {
                location, status, ..
            } => {
                let is_creative = self.player.game_mode == GameMode::Creative;
                if (is_creative && status == DiggingStatus::StartDigging)
                    || (!is_creative && status == DiggingStatus::FinishDigging)
                {
                    let block_state = self
                        .server
                        .world
                        .get_block(location.x, location.y, location.z);
                    if block_state != 0 {
                        self.change_block(location, 0).await?;
                        if !is_creative {
                            let block_id = block_id!(block_state);
                            let block_meta = block_meta!(block_state);

                            // Create item entity
                            let eid = self.server.new_id();
                            self.server
                                .send_broadcast(Packet::S0ESpawnObject {
                                    entity_id: eid,
                                    kind: 2,
                                    x: location.x as f32 + 0.5,
                                    y: location.y as f32 + 0.5,
                                    z: location.z as f32 + 0.5,
                                    pitch: 0.0,
                                    yaw: 0.0,
                                    data: 0,
                                })
                                .await?;

                            // Update item entity metadata
                            self.server
                                .send_broadcast(Packet::S1CEntityMeta {
                                    entity_id: eid,
                                    entries: vec![EntityMetaEntry::new(
                                        10,
                                        EntityMetaData::Slot(ItemStack {
                                            id: block_id as i16,
                                            count: 1,
                                            damage: block_meta,
                                        }),
                                    )],
                                })
                                .await?;
                        }
                    }
                }
            }
            Packet::C08PlayerBlockPlacement { location, face } => {
                if face != BlockFace::Special {
                    let block_state = self
                        .server
                        .world
                        .get_block(location.x, location.y, location.z);

                    // Tall grass is replaced, therefore the offset is ignored
                    let new_loc = if block_id!(block_state) == 31 {
                        location
                    } else {
                        location.offset(face)
                    };

                    // Set the corresponding block, if the held item allows it
                    let held_item_stack =
                        self.player.item_stack_in_hotbar(self.player.selected_slot);
                    if held_item_stack.is_present() && held_item_stack.is_block() {
                        let new_state = block_state!(held_item_stack.id, held_item_stack.damage);
                        self.change_block(new_loc, new_state).await?;
                    }
                }
            }
            Packet::C09HeldItemChange { slot } => {
                self.player.selected_slot = slot;
            }
            Packet::C0AAnimation { .. } => {}
            Packet::C10SetCreativeSlot { slot_id, item } => {
                debug!("Set slot {:?} to {:?}", slot_id, item);
                let stack = self.player.item_stack_at(slot_id);
                *stack = item;
            }
            _ => {
                trace!("Received unhandled packet: {:?}", packet);
            }
        }

        Ok(())
    }

    async fn handle_command(&mut self, command: &str) -> io::Result<()> {
        let result = self.exec_command(command).await;
        let message_opt = match result {
            Ok(str) => str,
            Err(str) => Some(format!("§cError: {}", str)),
        };
        if message_opt.is_some() {
            self.send_packet(chat_packet!(1, message_opt.unwrap()))
                .await
        } else {
            Ok(())
        }
    }

    async fn exec_command(&mut self, command: &str) -> Result<Option<String>, String> {
        let command = Command::parse(command);
        match command.name() {
            "help" => {
                let help_msg = indoc! {"
                == §aHelp§r ==
                §9 /help§r: Show command overview
                §9 /gm §7<mode>§r: Change gamemode
                §9 /flyspeed §7<speed>§r: Set flying speed multiplier
                §9 /walkspeed §7<speed>§r: Set walking speed multiplier
                "};
                return Ok(Some(help_msg.trim().to_string()));
            }
            "gm" => {
                self.change_game_mode(GameMode::from(command.arg::<u8>(0)?))
                    .await
                    .expect("Failed to change game mode");

                return Ok(Some(format!(
                    "Game mode changed to {:?}",
                    self.player.game_mode
                )));
            }
            "flyspeed" => {
                self.player.fly_speed = command.arg::<f32>(0)?;
                self.send_abilities()
                    .await
                    .expect("Failed to send abilities");
                return Ok(Some(format!(
                    "Flying speed changed to {}",
                    self.player.fly_speed
                )));
            }
            "walkspeed" => {
                self.player.walk_speed = command.arg::<f32>(0)?;
                self.send_abilities()
                    .await
                    .expect("Failed to send abilities");
                return Ok(Some(format!(
                    "Walking speed changed to {}",
                    self.player.walk_speed
                )));
            }
            _ => return Err(format!("{}: Unknown command.", command.name())),
        }
    }

    async fn change_block(&mut self, location: BlockPos, block_state: u16) -> io::Result<()> {
        self.server
            .world
            .set_block(location.x, location.y, location.z, block_state);
        self.server
            .send_broadcast(Packet::S23BlockChange {
                location,
                block_state,
            })
            .await
    }

    async fn change_game_mode(&mut self, game_mode: GameMode) -> io::Result<()> {
        self.player.game_mode = game_mode;
        self.send_packet(Packet::S2BChangeGameState {
            reason: GameStateReason::ChangeGameMode,
            value: game_mode as i32 as f32,
        })
        .await?;
        self.send_abilities().await?;
        self.server
            .send_broadcast(Packet::S38PlayerListItem {
                uuid: self.player.uuid,
                action: PlayerListItemAction::UpdateGameMode { game_mode },
            })
            .await?;
        Ok(())
    }

    async fn update_chunks(&mut self, center: ChunkPos) -> io::Result<()> {
        if self.current_chunk_pos != center {
            self.current_chunk_pos = center;

            let r = self.server.config.view_dist;
            self.server.gen.request_region(center.x, center.z, r);
            self.server.gen.await_region(center.x, center.z, r).await;
            self.send_chunks(center.x, center.z, r).await?;

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
                self.send_packet(Packet::S21ChunkData { x: r.x, z: r.z })
                    .await?;
                self.known_chunks.remove(&r);
            }
        }

        Ok(())
    }

    async fn send_packet(&mut self, packet: Packet) -> io::Result<()> {
        self.msg_stream.send(packet).await
    }

    async fn send_abilities(&mut self) -> io::Result<()> {
        self.send_packet(Packet::S39PlayerAbilities {
            flags: AbilityFlags::from_game_mode(self.player.game_mode),
            flying_speed: self.player.fly_speed,
            walking_speed: self.player.walk_speed,
        })
        .await
    }

    async fn send_chunks(&mut self, center_x: i32, center_z: i32, r: i32) -> io::Result<()> {
        let mut chunk_refs = Vec::<MutexChunkRef>::new();

        // Collect chunks to be sent
        for z in -r..=r {
            for x in -r..=r {
                let chunk_pos = ChunkPos::new(center_x + x, center_z + z);
                let chunk_opt = self.server.world.get_chunk(chunk_pos);
                if chunk_opt.is_some() && !self.known_chunks.contains(&chunk_pos) {
                    chunk_refs.push(chunk_opt.unwrap());
                    self.known_chunks.insert(chunk_pos);
                }
            }
        }

        // Split into packets
        let chunks_per_packet = 10;

        for packet_chunk_refs in chunk_refs.chunks(chunks_per_packet) {
            // Lock and copy chunks for the network
            let mut chunks = Vec::<Chunk>::new();
            for chunk_ref in packet_chunk_refs {
                chunks.push(chunk_ref.lock().unwrap().clone())
            }

            // Send chunks
            self.send_packet(Packet::S26MapChunkBulk {
                skylight: true,
                chunks,
            })
            .await?;
        }

        Ok(())
    }
}
