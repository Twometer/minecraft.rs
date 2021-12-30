use bytes::{Buf, BytesMut};
use rand::Rng;
use serde_derive::Deserialize;
use uuid::Uuid;

use crate::world::{BlockPos, ChunkPos};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum GameMode {
    Survival,
    Creative,
    Adventure,
    Spectator,
}

impl From<u8> for GameMode {
    fn from(val: u8) -> Self {
        match val {
            0 => GameMode::Survival,
            1 => GameMode::Creative,
            2 => GameMode::Adventure,
            3 => GameMode::Spectator,
            _ => panic!("Invalid game mode {}", val),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemStack {
    pub id: i16,
    pub count: u8,
    pub damage: u16,
}

impl Default for ItemStack {
    fn default() -> Self {
        Self {
            id: -1,
            count: 0,
            damage: 0,
        }
    }
}

impl ItemStack {
    pub fn read(buf: &mut BytesMut) -> ItemStack {
        let mut stack = ItemStack {
            id: buf.get_i16(),
            count: 0,
            damage: 0,
        };
        if stack.id != -1 {
            stack.count = buf.get_u8();
            stack.damage = buf.get_u16();
        }
        stack
    }

    pub fn is_present(&self) -> bool {
        self.id != -1
    }

    pub fn is_block(&self) -> bool {
        self.id <= 255
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec3d {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2f {
    pub x: f32,
    pub y: f32,
}

pub trait Entity {
    fn id(&self) -> i32;

    fn pos(&self) -> Vec3d;

    fn block_pos(&self) -> BlockPos {
        let pos = self.pos();
        BlockPos::from_pos(pos.x, pos.y, pos.z)
    }

    fn chunk_pos(&self) -> ChunkPos {
        let block_pos = self.block_pos();
        ChunkPos::from_block_pos(block_pos.x, block_pos.z)
    }

    fn set_pos(&mut self, pos: Vec3d);

    fn rot(&self) -> Vec2f;

    fn set_rot(&mut self, rot: Vec2f);
}

pub struct Player {
    pub eid: i32,
    pub uuid: Uuid,
    pub username: String,
    pub position: Vec3d,
    pub rotation: Vec2f,
    pub game_mode: GameMode,
    pub fly_speed: f32,
    pub walk_speed: f32,
    pub inventory: Vec<ItemStack>,
    pub selected_slot: i16,
}

impl Player {
    pub fn new(eid: i32, game_mode: GameMode) -> Player {
        Player {
            eid,
            uuid: Uuid::from_u128(rand::thread_rng().gen()),
            username: String::new(),
            position: Default::default(),
            rotation: Default::default(),
            game_mode,
            fly_speed: 0.05,
            walk_speed: 0.1,
            inventory: vec![ItemStack::default(); 45],
            selected_slot: 0,
        }
    }

    pub fn is_logged_in(&self) -> bool {
        !self.username.is_empty()
    }

    pub fn item_stack_at(&mut self, id: i16) -> &mut ItemStack {
        return &mut self.inventory[id as usize];
    }

    pub fn item_stack_in_hotbar(&mut self, id: i16) -> &mut ItemStack {
        self.item_stack_at(36 + id) // offset for hotbar is slot #36
    }
}

impl Entity for Player {
    fn id(&self) -> i32 {
        self.eid
    }

    fn pos(&self) -> Vec3d {
        self.position
    }

    fn set_pos(&mut self, pos: Vec3d) {
        self.position = pos
    }

    fn rot(&self) -> Vec2f {
        self.rotation
    }

    fn set_rot(&mut self, rot: Vec2f) {
        self.rotation = rot;
    }
}
