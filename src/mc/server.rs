use std::{
    cell::RefCell,
    sync::{Arc, Mutex},
};

use super::{MinecraftClient, WriteBuffer};

pub struct MinecraftServer {
    clients: Vec<Arc<Mutex<MinecraftClient>>>,
}

impl MinecraftServer {
    pub fn new() -> MinecraftServer {
        MinecraftServer {
            clients: Vec::<Arc<Mutex<MinecraftClient>>>::new(),
        }
    }

    pub fn broadcast(&self, packet_id: i32, packet_payload: &WriteBuffer) {
        for client in self.clients.iter() {
            let mut client = client.lock().unwrap();
            client.send_packet(packet_id, packet_payload);
        }
    }

    pub fn add_client(&mut self, client: Arc<Mutex<MinecraftClient>>) {
        self.clients.push(client);
    }

    pub fn remove_client(&mut self, client: Arc<Mutex<MinecraftClient>>) {}
}
