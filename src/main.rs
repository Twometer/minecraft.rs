mod mc;

use crate::mc::codec::MinecraftCodec;
use futures::StreamExt;
use log::{debug, error, info};
use tokio::net::{TcpListener, TcpStream};
use tokio_util::codec::Framed;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    pretty_env_logger::init();
    info!("Starting server...");

    let listener = TcpListener::bind("127.0.0.1:25565").await?;
    info!("Listener bound and ready");

    loop {
        let (stream, _) = listener.accept().await?;
        handle_client(stream);
    }
}

fn handle_client(stream: TcpStream) {
    tokio::spawn(async move {
        debug!("Client {:?} connected", stream.peer_addr().unwrap());

        let codec = MinecraftCodec::new();
        let mut framed = Framed::new(stream, codec);

        while let Some(f) = framed.next().await {
            match f {
                Ok(packet) => {
                    debug!("Received {:?}", packet);

                    match packet {
                        mc::proto::Packet::C00Handshake {
                            protocol_version,
                            server_address: _,
                            server_port: _,
                            next_state,
                        } => {
                            if protocol_version != 47 {
                                panic!("Unsupported protocol");
                            }

                            framed.codec_mut().change_state(next_state);
                        }
                        _ => {}
                    }
                }
                Err(e) => {
                    error!("Client error: {}", e);
                    break;
                }
            }
        }
    });
}
