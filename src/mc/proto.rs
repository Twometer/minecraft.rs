#[derive(Debug)]
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

#[derive(Debug)]
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
    S00KeepAlive {
        timestamp: i32,
    },
    S01JoinGame {
        entity_id: i32,
        gamemode: u8,
        dimension: u8,
        difficulty: u8,
        player_list_size: u8,
        world_type: String,
        reduced_debug_info: bool,
    },
    S08SetPlayerPosition {
        x: f64,
        y: f64,
        z: f64,
        yaw: f32,
        pitch: f32,
        flags: u8,
    },
}

impl Packet {
    pub fn id(&self) -> i32 {
        match self {
            Packet::C00Handshake { .. } => 0x00,
            Packet::C00StatusRequest { .. } => 0x00,
            Packet::C01StatusPing { .. } => 0x01,
            Packet::S00StatusResponse { .. } => 0x00,
            Packet::S01StatusPong { .. } => 0x01,
            Packet::C00LoginStart { .. } => 0x00,
            Packet::S02LoginSuccess { .. } => 0x02,
            Packet::S03LoginCompression { .. } => 0x03,
            Packet::C00KeepAlive { .. } => 0x00,
            Packet::C01ChatMessage { .. } => 0x01,
            Packet::S00KeepAlive { .. } => 0x00,
            Packet::S01JoinGame { .. } => 0x01,
            Packet::S08SetPlayerPosition { .. } => 0x08,
        }
    }
}
