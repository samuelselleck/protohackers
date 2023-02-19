use std::error::Error;
use std::io;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;
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
            let res = handle_connection(socket).await;
            if let Err(e) = res {
                println!("error handling connection: {}", e);
            }
            println!("connection closed");
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
            Err(_) => break,
        }
    }
    Ok(())
}

async fn handle_request_raw(
    reader: &mut BufReader<OwnedReadHalf>,
    writer: &mut OwnedWriteHalf,
) -> Result<bool> {
    tokio::io::copy(reader, writer).await?;
    Ok(false)
}
