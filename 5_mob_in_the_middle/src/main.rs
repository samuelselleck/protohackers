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
    let mut chat_r = BufReader::new(chat_r);
    let mut vic_r = BufReader::new(vic_r);

    println!("entered");
    loop {
        println!("mess");
        let mut buf_vic = Vec::new();
        let mut buf_chat = Vec::new();
        tokio::select! {
            //TODO next_line should not process the last line if return is pressed!
            res = vic_r.read_until(b'\n', &mut buf_vic) => {
                println!("{:?}", buf_vic);
                let n = res?;
                if n == 0 || buf_vic.last().unwrap() != &b'\n' {
                    break
                }
                let vic_message = String::from_utf8(buf_vic)?;
                println!("message from vic: {:?}", vic_message);
                    let mut ret = replace_bogus(vic_message);
                    ret.push('\n');
                    chat_w.write_all(ret.as_bytes()).await?;
            },
            res  = chat_r.read_until(b'\n', &mut buf_chat) => {
                let n = res?;
                if n == 0 || buf_chat.last().unwrap() != &b'\n' {
                    break
                }
                let chat_message = String::from_utf8(buf_chat)?;
                println!("message from server: {:?}", chat_message);
                    let mut ret = replace_bogus(chat_message);
                    ret.push('\n');
                    vic_w.write_all(ret.as_bytes()).await?;
            },
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
