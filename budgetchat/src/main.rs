use std::collections::HashSet;
use std::error::Error;
use std::io;
use std::sync::Arc;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::select;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender;
use tokio::sync::Mutex;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct ChatRoom {
    channel: Sender<String>,
    users: Arc<Mutex<HashSet<String>>>,
}

impl ChatRoom {
    fn new() -> Self {
        Self {
            channel: broadcast::channel(16).0,
            users: Default::default(),
        }
    }

    fn new_user(&mut self) -> ChatRoomConnection {
        let sender = self.channel.clone();
        let users = self.users.clone();
        ChatRoomConnection { sender, users }
    }
}

struct ChatRoomConnection {
    sender: Sender<String>,
    users: Arc<Mutex<HashSet<String>>>,
}

impl ChatRoomConnection {
    async fn handle_events(self, stream: TcpStream) -> Result<()> {
        let (reader, mut writer) = stream.into_split();
        let mut lines = BufReader::new(reader).lines();
        writer
            .write_all("Welcome to budgetchat! What shall I call you?\n".as_bytes())
            .await?;
        let name = lines
            .next_line()
            .await?
            .ok_or("connection closed before name specified")?;
        let correct = name.len() >= 1 && name.chars().all(|c| c.is_alphanumeric());
        if !correct {
            writer
                .write_all("please provide a valid name (at least 1 characater long, alpha-numeric characters.)\n".as_bytes())
                .await?;
            return Err("not valid name".into());
        }
        self.sender
            .send(format!("* {} joined the chat\n", &name))
            .unwrap_or_default();
        println!("{} joined the chat", &name);
        let mut reciever = self.sender.subscribe();
        let active_users = async {
            let mut users = self.users.lock().await;
            let active_users: Vec<_> = users.iter().cloned().collect();
            users.insert(name.clone());
            active_users
        }
        .await;
        writer
            .write_all(format!("* current users: {}\n", active_users.join(", ")).as_bytes())
            .await?;

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
        {
            let mut users = self.users.lock().await;
            users.remove(&name);
        }
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
