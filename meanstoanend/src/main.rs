use std::collections::BTreeMap;
use std::error::Error;
use std::io;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::io::BufReader;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpListener;
use tokio::net::TcpStream;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct PriceData {
    data: BTreeMap<i32, Vec<i32>>,
}

impl PriceData {
    fn new() -> Self {
        PriceData {
            data: Default::default(),
        }
    }

    fn add_record(&mut self, time: i32, price: i32) {
        self.data.entry(time).or_default().push(price);
    }

    fn calculate_mean(&self, start: i32, end: i32) -> i32 {
        if end < start {
            return 0;
        }
        let prices = self.data.range(start..=end).flat_map(|(_, v)| v.iter());
        let mut sum: i128 = 0;
        let mut count = 0;
        for &p in prices {
            sum += p as i128;
            count += 1;
        }
        if count == 0 {
            0
        } else {
            (sum / count) as i32
        }
    }
}

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
    let mut data = PriceData::new();
    loop {
        let mut buf = [0; 9];
        let n = reader.read_exact(&mut buf).await?;
        if n == 0 {
            break;
        };
        let res = handle_request_raw(&mut buf, &mut writer, &mut data).await;
        match res {
            Ok(true) => continue,
            Ok(false) => break,
            Err(_) => break,
        }
    }
    Ok(())
}

async fn handle_request_raw(
    buf: &[u8],
    writer: &mut OwnedWriteHalf,
    session_data: &mut PriceData,
) -> Result<bool> {
    //process packet and update local state
    let message_type = buf[0] as char;
    let first = i32::from_be_bytes((&buf[1..5]).try_into()?);
    let second = i32::from_be_bytes((&buf[5..]).try_into()?);
    match message_type {
        'I' => {
            session_data.add_record(first, second);
        }
        'Q' => {
            let mean = session_data.calculate_mean(first, second);
            writer.write_i32(mean).await?;
        }
        _ => return Err("unsupported operation".into()),
    }

    Ok(true)
}
