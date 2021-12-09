use super::MinecraftClient;

pub struct MinecraftServer {
    clients: Vec<MinecraftClient>,
}

impl MinecraftServer {
    pub fn new() -> MinecraftServer {
        MinecraftServer {
            clients: Vec::<MinecraftClient>::new(),
        }
    }
}
