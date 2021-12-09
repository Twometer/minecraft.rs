use std::{cell::RefCell, sync::Mutex};

use super::{MinecraftClient, WriteBuffer};

pub struct MinecraftServer {
    clients: Vec<Mutex<MinecraftClient>>,
}

impl MinecraftServer {
    pub fn new() -> MinecraftServer {
        MinecraftServer {
            clients: Vec::<Mutex<MinecraftClient>>::new(),
        }
    }

    pub fn broadcast(&self, packet_id: i32, packet_payload: &WriteBuffer) {}

    pub fn add_client(&mut self, client: &MinecraftClient) {}

    pub fn remove_client(&mut self, client: &MinecraftClient) {}
}
