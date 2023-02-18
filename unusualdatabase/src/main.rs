use std::{collections::HashMap, error::Error};
use tokio::net::UdpSocket;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::main]
async fn main() -> Result<()> {
    let socket = UdpSocket::bind("0.0.0.0:8080").await?;
    let mut storage = HashMap::<String, String>::new();
    storage.insert("version".into(), "samuels KV store v 1.0".into());

    loop {
        let mut buf = [0; 1024];
        // println!("waiting for recieve...");
        let (n, sender) = socket.recv_from(&mut buf).await?;
        let msg = String::from_utf8(buf[..n].to_vec())?;
        // println!("recieved: {:?}", &msg);
        let ind = msg.find('=');
        match ind {
            Some(i) => {
                let (key, value) = msg.split_at(i);
                // println!("inserting {:?} = {:?}", key, &value[1..]);
                if key == "version" {
                    continue;
                }
                storage.insert(key.into(), value[1..].into());
            }
            None => {
                // println!("message: {:?}", msg);
                if let Some(v) = storage.get(msg.as_str()) {
                    let response = format!("{}={}", msg, v);
                    // println!("sending response: {:?}", response);
                    socket.send_to(response.as_bytes(), sender).await?;
                } else {
                    // println!("no such key");
                }
            }
        }
    }
}
