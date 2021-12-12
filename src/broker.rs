use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::mc::proto::Packet;

pub struct PacketBroker {
    broadcast: mpsc::Sender<Packet>,
    outputs: Arc<Mutex<Vec<mpsc::Sender<Packet>>>>,
}

impl PacketBroker {
    pub fn new() -> PacketBroker {
        let (tx, rx) = mpsc::channel::<Packet>(128);
        let outputs = Arc::new(Mutex::new(Vec::<mpsc::Sender<Packet>>::new()));

        Self::spawn_broadcaster(rx, outputs.clone());

        PacketBroker {
            broadcast: tx,
            outputs,
        }
    }

    pub fn new_broadcast(&mut self) -> mpsc::Sender<Packet> {
        self.broadcast.clone()
    }

    pub async fn new_unicast(&mut self) -> mpsc::Receiver<Packet> {
        let mut channels = self.outputs.lock().await;
        let (tx, rx) = mpsc::channel::<Packet>(128);
        channels.push(tx);
        rx
    }

    fn spawn_broadcaster(
        mut input: mpsc::Receiver<Packet>,
        outputs: Arc<Mutex<Vec<mpsc::Sender<Packet>>>>,
    ) {
        tokio::spawn(async move {
            while let Some(packet) = input.recv().await {
                let outputs = outputs.lock().await;
                let send_futures: Vec<_> = outputs
                    .iter()
                    .map(|output| output.send(packet.clone()))
                    .collect();

                futures::future::join_all(send_futures).await;
            }
        });
    }
}
