use std::io;
use std::io::ErrorKind;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

type Result<T> = io::Result<T>;

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    let mut counter: i32 = 0;
    loop {
        let (socket, _) = listener.accept().await?;
        counter += 1;
        let id = counter;
        tokio::spawn(async move {
            println!("connection established");
            let _ = replacing_proxy(socket, id).await;
            println!("connection closed");
        });
    }
}

async fn replacing_proxy(victim_connection: TcpStream, id: i32) -> Result<()> {
    let addr = "chat.protohackers.com:16963";
    let chat_connection = TcpStream::connect(addr).await?;
    let (vic_r, mut vic_w) = victim_connection.into_split();
    let (chat_r, mut chat_w) = chat_connection.into_split();
    let mut chat_r = BufReader::new(chat_r);
    let mut vic_r = BufReader::new(vic_r);
    let mut vic_buf = Vec::new();
    let mut chat_buf = Vec::new();
    loop {
        tokio::select! {
            res = read_line(id, "victim", &mut vic_r, &mut vic_buf) => {
                write_bogused_line(id, "chat server", &mut chat_w, res?).await?;
            },
            res = read_line(id, "chat server", &mut chat_r, &mut chat_buf) => {
                write_bogused_line(id, "victim", &mut vic_w, res?).await?;
            }
        }
    }
}

async fn read_line(
    connection_id: i32,
    from_ident: &str,
    from: &mut BufReader<OwnedReadHalf>,
    buf: &mut Vec<u8>,
) -> Result<String> {
    from.read_until(b'\n', buf).await?;
    buf.last()
        .filter(|&end| end != &b'\n')
        .ok_or(io::Error::new(ErrorKind::Other, "end"))?;
    let message =
        String::from_utf8(buf.clone()).map_err(|e| io::Error::new(ErrorKind::InvalidData, e))?;
    println!(
        "{}: recv ({: <12}): {:?}",
        connection_id, from_ident, message
    );
    buf.clear();
    Ok(message)
}

async fn write_bogused_line(
    id: i32,
    to_ident: &str,
    to: &mut OwnedWriteHalf,
    message: String,
) -> Result<()> {
    let mut ret = replace_bogus(message);
    ret.push('\n');
    println!("{}: sent ({: <12}): {:?}", id, to_ident, ret);
    to.write_all(ret.as_bytes()).await?;
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
