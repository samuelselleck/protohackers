use std::error::Error;
use std::io;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(async {
            println!("connection established");
            replacing_proxy(socket).await.expect("closed unexpectedly");
            println!("connection closed");
        });
    }
}

async fn replacing_proxy(victim_connection: TcpStream) -> Result<()> {
    let addr = "chat.protohackers.com:16963";
    let chat_connection = TcpStream::connect(addr).await?;
    let (vic_r, mut vic_w) = victim_connection.into_split();
    let (chat_r, mut chat_w) = chat_connection.into_split();
    let mut chat_r = BufReader::new(chat_r).lines();
    let mut vic_r = BufReader::new(vic_r).lines();

    println!("entered");
    let mut buf_vic = Vec::new();
    let mut buf_chat = Vec::new();
    loop {
        tokio::select! {
            //TODO next_line should not process the last line if return is pressed!
            Ok(vic_message) = vic_r.next_line() => {
                println!("message from vic: {:?}", vic_message);
                if let Some(vic_message) = vic_message {
                    let mut ret = replace_bogus(vic_message);
                    ret.push('\n');
                    chat_w.write_all(ret.as_bytes()).await?;
                } else {
                    break
                }
            },
            Ok(chat_message) = chat_r.next_line() => {
                println!("message from server: {:?}", chat_message);
                if let Some(chat_message) = chat_message {
                    let mut ret = replace_bogus(chat_message);
                    ret.push('\n');
                    vic_w.write_all(ret.as_bytes()).await?;
                } else {
                    break
                }
            },
            else => {

            }
        }
    }
    Ok(())
}

fn replace_bogus(s: String) -> String {
    s.split_whitespace()
        .map(|w| {
            if w.starts_with('7')
                && w.len() >= 26
                && w.len() <= 35
                && w.chars().all(|c| c.is_alphanumeric())
            {
                "7YWHMfk9JZe0LM0g1ZauHuiSxhI".into()
            } else {
                w.into()
            }
        })
        .collect::<Vec<String>>()
        .join(" ")
}
