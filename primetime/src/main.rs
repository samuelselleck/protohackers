use serde::{Deserialize, Serialize};
use serde_json;
use std::error::Error;
use std::io;
use tokio::io::AsyncBufReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Debug, Serialize, Deserialize)]
struct Response {
    method: String,
    prime: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Request {
    method: String,
    prime: serde_json::value::Number,
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        tokio::spawn(async {
            let res = handle_connection(socket).await;
            if let Err(e) = res {
                println!("error handling connection: {}", e);
            }
        });
    }
}

async fn handle_connection(socket: TcpStream) -> Result<()> {
    let (reader, mut writer) = socket.into_split();
    let mut reader = BufReader::new(reader);
    loop {
        let res = handle_request_raw(&mut reader, &mut writer).await;
        match res {
            Ok(true) => continue,
            Ok(false) => break,
            Err(e) => {
                println!("error handling request: {}", e);
            }
        }
    }
    Ok(())
}

async fn handle_request_raw(
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
) -> Result<bool> {
    let mut buf = Vec::new();
    let n = reader.read_until(b'\n', &mut buf).await?;
    if n == 0 {
        return Ok(false);
    }
    let parsed = serde_json::from_slice::<Request>(&buf)
        .ok()
        .filter(|r| r.method == "isPrime");
    match parsed {
        Some(request) => {
            let mut response = serde_json::to_string(&Response {
                method: "isPrime".to_string(),
                prime: is_prime(request.prime),
            })?;
            response.push('\n');
            writer.write_all(&response.as_bytes()).await?;
            Ok(true)
        }
        None => {
            writer.write_all(b"malformed!\n").await?;
            Ok(false)
        }
    }
}

fn is_prime(num: serde_json::value::Number) -> bool {
    let num: Option<u64> = num.as_u64();
    match num {
        Some(n) => (2..n).all(|v| n % v != 0),
        None => false,
    }
}