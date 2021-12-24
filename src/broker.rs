use std::sync::Arc;

use tokio::sync::{mpsc, Mutex};

use crate::mc::proto::Packet;

pub struct PacketBroker {
    broadcast_tx: mpsc::Sender<Packet>,
    client_streams: Arc<Mutex<Vec<mpsc::Sender<Packet>>>>,
}

impl PacketBroker {
    pub fn new() -> PacketBroker {
        let (broadcast_tx, broadcast_rx) = mpsc::channel::<Packet>(128);
        let clients = Arc::new(Mutex::new(Vec::<mpsc::Sender<Packet>>::new()));

        let broker = PacketBroker {
            broadcast_tx,
            client_streams: clients,
        };
        broker.start(broadcast_rx);
        broker
    }

    pub fn new_broadcast(&self) -> mpsc::Sender<Packet> {
        self.broadcast_tx.clone()
    }

    pub async fn new_unicast(&self) -> mpsc::Receiver<Packet> {
        let mut channels = self.client_streams.lock().await;
        let (tx, rx) = mpsc::channel::<Packet>(128);
        channels.push(tx);
        rx
    }

    fn start(&self, mut broadcast_rx: mpsc::Receiver<Packet>) {
        let clients = self.client_streams.clone();
        tokio::spawn(async move {
            while let Some(packet) = broadcast_rx.recv().await {
                let outputs = clients.lock().await;
                let send_futures: Vec<_> = outputs
                    .iter()
                    .map(|output| output.send(packet.clone()))
                    .collect();

                futures::future::join_all(send_futures).await;
            }
        });
    }
}
