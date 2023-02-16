use std::error::Error;
use std::io;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Receiver;
use tokio::sync::broadcast::Sender;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct ChatRoom {
    channel: Sender<String>,
}

impl ChatRoom {
    fn new() -> Self {
        Self {
            channel: broadcast::channel(16).0,
        }
    }

    fn new_user(&mut self) -> ChatRoomConnection {
        let sender = self.channel.clone();
        ChatRoomConnection { sender }
    }
}

struct ChatRoomConnection {
    sender: Sender<String>,
}

impl ChatRoomConnection {
    async fn handle_events(self, stream: TcpStream) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        let name = lines
            .next_line()
            .await?
            .ok_or("connection closed before name specified")?;
        //TODO verify name has valid symbols (small/large letters and numbers) and length >= 1
        self.sender
            .send(format!("* {} joined the chat\n", &name))
            .unwrap_or_default();
        println!("{} joined the chat", &name);
        let mut reciever = self.sender.subscribe();
        loop {
            select! {
                Ok(l) = lines.next_line() => {
                    if let Some(m) = l {
                        println!("recieved message from client {}: {}", &name,  m);
                        self.sender.send(format!("[{}]: {}\n", &name, m))?;
                    } else {
                        break
                    }
                },
                Ok(m) = reciever.recv() => {
                    if !m.contains(&name) {
                        println!("sending broadcasted message to {}: {}", &name, m);
                        writer.write_all(m.as_bytes()).await?
                    }
                }
            }
        }
        println!("{} left the chat", &name);
        self.sender.send(format!("* {} left the chat\n", &name))?;
        Ok(())
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    let mut room = ChatRoom::new();
    loop {
        let (socket, _) = listener.accept().await?;
        let connection = room.new_user();
        tokio::spawn(async {
            println!("connection established");
            connection
                .handle_events(socket)
                .await
                .expect("handler closed unexpectedly");
            println!("connection closed");
        });
    }
}
