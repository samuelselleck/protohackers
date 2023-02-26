mod messages;
mod roadnetwork;
use futures::sink::SinkExt;
use futures::StreamExt;
use messages::*;
use roadnetwork::RoadNetwork;
use roadnetwork::Sighting;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio_util::codec::Framed;

use crate::roadnetwork::CHANNEL_SIZE;

#[tokio::main]
async fn main() -> io::Result<()> {
    let network = Arc::<RoadNetwork>::default();
    let listener = TcpListener::bind("0.0.0.0:8080").await?;
    let mut counter: i32 = 0;
    println!("server started");

    loop {
        let (socket, _) = listener.accept().await?;
        println!("connecting...");
        let network = network.clone();
        let id = counter;
        counter += 1;
        tokio::spawn(async move {
            println!("{id} connection established");
            let stream_and_sink = Framed::new(socket, messages::MessageCodec::new());
            let (sink, stream) = stream_and_sink.split();

            let (tx_heartbeat, rx_heartbeat) = mpsc::channel::<Duration>(CHANNEL_SIZE);
            let (tx_main, rx_main) = mpsc::channel::<ClientMessage>(CHANNEL_SIZE);
            let (tx_out, rx_out) = mpsc::channel::<ServerMessage>(CHANNEL_SIZE);

            //coordinate splitting heartbeat and normal messages
            let tx_out_s = tx_out.clone();
            tokio::spawn(async move {
                split_messages(id, stream, tx_heartbeat, tx_main, tx_out_s).await
            });

            //send messages from output channel to sink, close in case of error message
            tokio::spawn(async move { pipe_output(id, rx_out, sink).await });

            //handle heart beats
            let tx_out_h = tx_out.clone();
            tokio::spawn(async move { handle_heartbeat(rx_heartbeat, tx_out_h).await });

            //handle camera/dispatcher messages
            let tx_out_m = tx_out.clone();
            tokio::spawn(async move { handle_main(id, network, rx_main, tx_out_m).await });

            println!("{id} connection closed");
        });
    }
}

async fn pipe_output(
    id: i32,
    mut rx_out: Receiver<ServerMessage>,
    mut sink: impl SinkExt<ServerMessage> + Unpin,
) -> Option<()> {
    while let Some(out) = rx_out.recv().await {
        let is_err = matches!(out, ServerMessage::Error(_));
        sink.send(out).await.ok()?;
        if is_err {
            sink.close().await.ok()?;
            break;
        }
    }
    println!("{id} output closed");
    Some(())
}

async fn split_messages(
    id: i32,
    mut stream: impl StreamExt<Item = io::Result<ClientMessage>> + Unpin,
    tx_heartbeat: Sender<Duration>,
    tx_main: Sender<ClientMessage>,
    tx_out: Sender<ServerMessage>,
) -> Option<()> {
    while let Some(message) = stream.next().await {
        match message {
            Ok(ClientMessage::HeartbeatRequest(interval)) => {
                tx_heartbeat.send(interval).await.ok()?;
            }
            Ok(message) => {
                tx_main.send(message).await.ok()?;
            }
            Err(_) => {
                tx_out
                    .send(ServerMessage::Error("not valid message type".into()))
                    .await
                    .ok()?;
            }
        }
    }
    println!("{id} client disconnected");
    Some(())
}

async fn handle_heartbeat(
    mut rx_heartbeat: Receiver<Duration>,
    tx_out_heart: Sender<ServerMessage>,
) -> Option<()> {
    let interval = rx_heartbeat.recv().await.expect("never recieved heartbeat");
    let tx_out_heart_inner = tx_out_heart.clone();
    if !interval.is_zero() {
        tokio::spawn(async move {
            loop {
                if tx_out_heart.send(ServerMessage::HeartBeat).await.is_err() {
                    break;
                };
                tokio::time::sleep(interval).await;
            }
        });
    }
    if let Some(_) = rx_heartbeat.recv().await {
        tx_out_heart_inner
            .send(ServerMessage::Error("recieved second heartbeat".into()))
            .await
            .ok()?;
    };
    Some(())
}

async fn handle_main(
    id: i32,
    network: Arc<RoadNetwork>,
    mut rx_main: Receiver<ClientMessage>,
    tx_out_main: Sender<ServerMessage>,
) -> Option<()> {
    match rx_main.recv().await? {
        ClientMessage::IAmCamera {
            road,
            position,
            speed_limit,
        } => {
            let tx_road = network.register_camera(road, speed_limit);
            handle_camera(
                id,
                road,
                position,
                speed_limit,
                rx_main,
                tx_out_main,
                tx_road,
            )
            .await?;
        }
        ClientMessage::IAmDispatcher { roads } => {
            let tx_out = tx_out_main.clone();
            tokio::spawn(async move {
                let _ = rx_main.recv().await;
                let _ = tx_out
                    .send(ServerMessage::Error(
                        "recieved second displacher disconnect message".into(),
                    ))
                    .await;
            });
            let stream = network.register_dispatcher(roads);
            handle_dispatcher(id, stream, tx_out_main).await;
        }
        _ => {
            tx_out_main
                .send(ServerMessage::Error(
                    "expected (camera/dispatcher) initialization message.".into(),
                ))
                .await
                .ok()?;
        }
    };
    Some(())
}

async fn handle_dispatcher(
    id: i32,
    mut stream: impl StreamExt<Item = ServerMessage> + Unpin,
    tx_out: Sender<ServerMessage>,
) -> Option<()> {
    while let Some(msg) = stream.next().await {
        tx_out.send(msg).await.ok()?
    }
    println!("{id} dispatcher disconnected");
    Some(())
}

async fn handle_camera(
    id: i32,
    road: RoadID,
    position: Mile,
    speed_limit: Speed,
    mut rx_main: Receiver<ClientMessage>,
    tx_out: Sender<ServerMessage>,
    tx_road: Sender<Sighting>,
) -> Option<()> {
    while let Some(msg) = rx_main.recv().await {
        match msg {
            ClientMessage::PlateDetected { plate, time } => {
                println!(
                    "{id} camera (road = {road}, mile = {position}, \
                     speed limit = {speed_limit}) sighted {plate} at {time}"
                );
                tx_road
                    .send(Sighting {
                        road,
                        speed_limit,
                        plate,
                        position,
                        time,
                    })
                    .await
                    .ok()?;
            }
            _ => {
                eprintln!("{id} camera got unexpected message");
                tx_out
                    .send(ServerMessage::Error(
                        "recived unextected message as camera".into(),
                    ))
                    .await
                    .ok()?;
            }
        }
    }
    Some(())
}
