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
