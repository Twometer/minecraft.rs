use crate::{
    model::GameMode,
    world::{BlockPos, Chunk},
};

#[derive(Debug, Clone)]
pub enum PlayState {
    Handshake,
    Status,
    Login,
    Play,
}

impl TryFrom<i32> for PlayState {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            1 => Ok(PlayState::Status),
            2 => Ok(PlayState::Login),
            _ => Err(()),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum EntityMetaData {
    Byte(u8),
    Short(i16),
    Int(i32),
    Float(f32),
    String(String),
    Slot { id: u16, count: u8, damage: u16 },
    Vec3i { x: i32, y: i32, z: i32 },
    Vec3f { x: f32, y: f32, z: f32 },
}

impl EntityMetaData {
    pub fn type_id(&self) -> u8 {
        match self {
            &EntityMetaData::Byte(_) => 0,
            &EntityMetaData::Short(_) => 1,
            &EntityMetaData::Int(_) => 2,
            &EntityMetaData::Float(_) => 3,
            &EntityMetaData::String(_) => 4,
            &EntityMetaData::Slot { .. } => 5,
            &EntityMetaData::Vec3i { .. } => 6,
            &EntityMetaData::Vec3f { .. } => 7,
        }
    }
}

#[derive(Debug, Clone)]
pub struct EntityMetaEntry {
    pub index: u8,
    pub data: EntityMetaData,
}

impl EntityMetaEntry {
    pub fn new(index: u8, data: EntityMetaData) -> EntityMetaEntry {
        EntityMetaEntry { index, data }
    }
}

#[derive(Debug, Clone)]
pub struct AbilityFlags {
    pub allow_flying: bool,
    pub is_creative: bool,
    pub is_flying: bool,
    pub god_mode: bool,
}

impl AbilityFlags {
    pub fn new(
        allow_flying: bool,
        is_creative: bool,
        is_flying: bool,
        god_mode: bool,
    ) -> AbilityFlags {
        AbilityFlags {
            allow_flying,
            is_creative,
            is_flying,
            god_mode,
        }
    }
    pub fn from_game_mode(game_mode: GameMode) -> AbilityFlags {
        match game_mode {
            GameMode::Survival => AbilityFlags::new(false, false, false, false),
            GameMode::Creative => AbilityFlags::new(true, true, false, true),
            GameMode::Adventure => AbilityFlags::new(false, false, false, false),
            GameMode::Spectator => AbilityFlags::new(true, false, true, true),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PlayerListItemAction {
    AddPlayer {
        name: String,
        game_mode: GameMode,
        ping: i32,
        display_name: Option<String>,
    },
    UpdateGameMode {
        game_mode: GameMode,
    },
    UpdateLatency {
        ping: i32,
    },
    UpdateDisplayName {
        display_name: Option<String>,
    },
    RemovePlayer,
}

impl PlayerListItemAction {
    pub fn id(&self) -> i32 {
        match self {
            Self::AddPlayer { .. } => 0,
            Self::UpdateGameMode { .. } => 1,
            Self::UpdateLatency { .. } => 2,
            Self::UpdateDisplayName { .. } => 3,
            Self::RemovePlayer { .. } => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiggingStatus {
    StartDigging,
    CancelDigging,
    FinishDigging,
    DropStack,
    DropItem,
    FinishAction,
}

impl From<u8> for DiggingStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => DiggingStatus::StartDigging,
            1 => DiggingStatus::CancelDigging,
            2 => DiggingStatus::FinishDigging,
            3 => DiggingStatus::DropStack,
            4 => DiggingStatus::DropItem,
            5 => DiggingStatus::FinishAction,
            _ => panic!("Unknown digging status {}", value),
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum GameStateReason {
    InvalidBed,
    EndRaining,
    BeginRaining,
    ChangeGameMode,
    EnterCredits,
    DemoMessage,
    ArrowHitting,
    FadeValue,
    FadeTime,
    MobAppearance,
}

#[derive(Debug, Clone)]
pub enum Packet {
    // State::Handshake
    C00Handshake {
        protocol_version: i32,
        server_address: String,
        server_port: u16,
        next_state: PlayState,
    },

    // State::Status
    C00StatusRequest,
    C01StatusPing {
        timestamp: i64,
    },
    S00StatusResponse {
        status: String,
    },
    S01StatusPong {
        timestamp: i64,
    },

    // State::Login
    C00LoginStart {
        username: String,
    },
    S02LoginSuccess {
        uuid: String,
        username: String,
    },
    S03LoginCompression {
        threshold: i32,
    },

    // State::Play
    C00KeepAlive {
        id: i32,
    },
    C01ChatMessage {
        message: String,
    },
    C03Player {
        on_ground: bool,
    },
    C04PlayerPos {
        x: f64,
        y: f64,
        z: f64,
        on_ground: bool,
    },
    C05PlayerRot {
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    },
    C06PlayerPosRot {
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        on_ground: bool,
    },
    C07PlayerDigging {
        status: DiggingStatus,
        location: BlockPos,
        face: u8,
    },
    S00KeepAlive {
        timestamp: i32,
    },
    S01JoinGame {
        entity_id: i32,
        game_mode: GameMode,
        dimension: u8,
        difficulty: u8,
        player_list_size: u8,
        world_type: String,
        reduced_debug_info: bool,
    },
    S02ChatMessage {
        json_data: String,
        position: u8,
    },
    S08SetPlayerPosition {
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        flags: u8,
    },
    S21ChunkData {
        x: i32,
        z: i32,
    },
    S26MapChunkBulk {
        skylight: bool,
        chunks: Vec<Chunk>,
    },
    S0ESpawnObject {
        entity_id: i32,
        kind: u8,
        x: f32,
        y: f32,
        z: f32,
        pitch: f32,
        yaw: f32,
        data: i32,
    },
    S1CEntityMeta {
        entity_id: i32,
        entries: Vec<EntityMetaEntry>,
    },
    S2BChangeGameState {
        reason: GameStateReason,
        value: f32,
    },
    S38PlayerListItem {
        uuid: uuid::Uuid,
        action: PlayerListItemAction,
    },
    S39PlayerAbilities {
        flags: AbilityFlags,
        flying_speed: f32,
        walking_speed: f32,
    },
}

impl Packet {
    pub fn id(&self) -> i32 {
        match self {
            &Packet::C00Handshake { .. } => 0x00,

            &Packet::C00StatusRequest { .. } => 0x00,
            &Packet::C01StatusPing { .. } => 0x01,
            &Packet::S00StatusResponse { .. } => 0x00,
            &Packet::S01StatusPong { .. } => 0x01,

            &Packet::C00LoginStart { .. } => 0x00,
            &Packet::S02LoginSuccess { .. } => 0x02,
            &Packet::S03LoginCompression { .. } => 0x03,

            &Packet::C00KeepAlive { .. } => 0x00,
            &Packet::C01ChatMessage { .. } => 0x01,
            &Packet::C03Player { .. } => 0x03,
            &Packet::C04PlayerPos { .. } => 0x04,
            &Packet::C05PlayerRot { .. } => 0x05,
            &Packet::C06PlayerPosRot { .. } => 0x06,
            &Packet::C07PlayerDigging { .. } => 0x07,
            &Packet::S00KeepAlive { .. } => 0x00,
            &Packet::S01JoinGame { .. } => 0x01,
            &Packet::S02ChatMessage { .. } => 0x02,
            &Packet::S08SetPlayerPosition { .. } => 0x08,
            &Packet::S1CEntityMeta { .. } => 0x1C,
            &Packet::S21ChunkData { .. } => 0x21,
            &Packet::S26MapChunkBulk { .. } => 0x26,
            &Packet::S0ESpawnObject { .. } => 0x0E,
            &Packet::S2BChangeGameState { .. } => 0x2B,
            &Packet::S38PlayerListItem { .. } => 0x38,
            &Packet::S39PlayerAbilities { .. } => 0x39,
        }
    }
}
