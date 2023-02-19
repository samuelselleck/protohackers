use regex::Regex;
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
    let regex = Regex::new(r"(^|\s)(7[a-zA-Z0-9]{25, 34})($|\s)").unwrap();
    loop {
        let (socket, _) = listener.accept().await?;
        let rgx = regex.clone();
        tokio::spawn(async {
            println!("connection established");
            replacing_proxy(socket, rgx, "7YWHMfk9JZe0LM0g1ZauHuiSxhI")
                .await
                .expect("closed unexpectedly");
            println!("connection closed");
        });
    }
}

async fn replacing_proxy(
    victim_connection: TcpStream,
    regex: Regex,
    replacement: &str,
) -> Result<()> {
    let addr = "chat.protohackers.com:16963";
    let chat_connection = TcpStream::connect(addr).await?;
    let (vic_r, mut vic_w) = victim_connection.into_split();
    let (chat_r, mut chat_w) = chat_connection.into_split();
    let mut chat_r = BufReader::new(chat_r).lines();
    let mut vic_r = BufReader::new(vic_r).lines();

    println!("entered");
    loop {
        tokio::select! {
            Ok(vic_message) = vic_r.next_line() => {
                println!("message from vic: {:?}", vic_message);
                if let Some(vic_message) = vic_message {
                    let mut ret = regex.replace_all(&vic_message, replacement);
                    ret.to_mut().push('\n');
                    chat_w.write_all(ret.as_bytes()).await?;
                } else {
                    break
                }
            },
            Ok(chat_message) = chat_r.next_line() => {
                println!("message from server: {:?}", chat_message);
                if let Some(chat_message) = chat_message {
                    let mut ret = regex.replace_all(&chat_message, replacement);
                    ret.to_mut().push('\n');
                    vic_w.write_all(ret.as_bytes()).await?;
                } else {
                    break
                }
            },
        }
    }
    Ok(())
}
