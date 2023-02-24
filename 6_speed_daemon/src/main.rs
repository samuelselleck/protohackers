mod messages;

use dashmap::DashMap;
use futures::sink::SinkExt;
use futures::stream::select_all;
use futures::StreamExt;
use messages::*;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio::sync::mpsc::Receiver;
use tokio::sync::mpsc::Sender;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::codec::Framed;

const CHANNEL_SIZE: usize = 2048;

#[derive(Debug)]
struct Sighting {
    road_id: RoadID,
    road_speed_limit: Speed,
    plate: Plate,
    mile: Mile,
    time: TimeStamp,
}

#[derive(Default)]
struct RoadNetwork {
    roads: DashMap<RoadID, Road>,
}

struct Road {
    speed_limit: Option<Speed>,
    rx_road: Option<Receiver<Sighting>>,
    tx_road: Sender<Sighting>,
}

impl Default for Road {
    fn default() -> Self {
        let (tx_road, rx_road) = mpsc::channel(CHANNEL_SIZE);
        Road {
            speed_limit: None,
            rx_road: Some(rx_road),
            tx_road,
        }
    }
}

impl RoadNetwork {
    fn register_camera(&self, road: RoadID, speed_limit: Speed) -> Sender<Sighting> {
        let mut road = self.roads.entry(road).or_default();
        road.speed_limit = Some(speed_limit);
        road.tx_road.clone()
    }

    fn register_dispatcher(&self, roads: Vec<RoadID>) -> impl StreamExt<Item = Sighting> + Unpin {
        let mut streams = Vec::new();
        for road in roads {
            let mut road = self.roads.entry(road).or_default();
            if let Some(rx_road) = road.rx_road.take() {
                let stream = ReceiverStream::new(rx_road);
                streams.push(stream);
            }
        }
        select_all(streams)
    }
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let network = Arc::<RoadNetwork>::default();
    let listener = TcpListener::bind("0.0.0.0:8080").await?;

    loop {
        let (socket, _) = listener.accept().await?;
        let network = network.clone();
        tokio::spawn(async {
            println!("connection established");
            let stream_and_sink = Framed::new(socket, messages::MessageCodec::new());
            let (mut sink, mut stream) = stream_and_sink.split();

            let (tx_heartbeat, mut rx_heartbeat) = mpsc::channel::<Duration>(CHANNEL_SIZE);
            let (tx_main, mut rx_main) = mpsc::channel::<ClientMessage>(CHANNEL_SIZE);
            let (tx_out, mut rx_out) = mpsc::channel::<ServerMessage>(CHANNEL_SIZE);

            //coordinate splitting heartbeat and normal messages
            tokio::spawn(async move {
                while let Some(Ok(message)) = stream.next().await {
                    if let ClientMessage::HeartbeatRequest(interval) = message {
                        tx_heartbeat.send(interval).await.unwrap();
                    } else {
                        tx_main.send(message).await.unwrap();
                    }
                }
            });

            //send messages from output channel to sink, close in case of error message
            tokio::spawn(async move {
                while let Some(out) = rx_out.recv().await {
                    let is_err = matches!(out, ServerMessage::Error(_));
                    sink.send(out).await.unwrap();
                    if is_err {
                        sink.close().await.unwrap();
                    }
                }
            });

            //handle heart beats
            let tx_out_heart = tx_out.clone();
            tokio::spawn(async move {
                let interval = rx_heartbeat.recv().await.expect("never recieved heartbeat");
                let tx_out_heart_inner = tx_out_heart.clone();
                tokio::spawn(async move {
                    loop {
                        if tx_out_heart.send(ServerMessage::HeartBeat).await.is_err() {
                            break;
                        };
                        tokio::time::sleep(interval).await;
                    }
                });
                if let Some(_) = rx_heartbeat.recv().await {
                    tx_out_heart_inner
                        .send(ServerMessage::Error("recieved second heartbeat".into()))
                        .await
                        .unwrap();
                }
            });

            //handle camera/dispatcher messages
            let tx_out_main = tx_out.clone();
            tokio::spawn(async move {
                match rx_main.recv().await.unwrap() {
                    ClientMessage::IAmCamera {
                        road,
                        position,
                        speed_limit,
                    } => {
                        let tx_road = network.register_camera(road, speed_limit);
                        handle_camera(road, position, speed_limit, rx_main, tx_out_main, tx_road)
                            .await;
                    }
                    ClientMessage::IAmDispatcher { roads } => {
                        let stream = network.register_dispatcher(roads);
                        handle_dispatcher(stream, tx_out_main).await;
                    }
                    _ => {
                        tx_out_main
                            .send(ServerMessage::Error(
                                "expected (camera/dispatcher) initialization message.".into(),
                            ))
                            .await
                            .unwrap();
                    }
                };
            });

            println!("connection closed");
        });
    }
}

async fn handle_dispatcher(
    mut stream: impl StreamExt<Item = Sighting> + Unpin,
    tx_out: Sender<ServerMessage>,
) {
    let mut log = HashMap::<RoadID, HashMap<Plate, Vec<(Mile, TimeStamp)>>>::new();
    while let Some(Sighting {
        road_id,
        road_speed_limit,
        plate,
        mile,
        time,
    }) = stream.next().await
    {
        let entries = log
            .entry(road_id)
            .or_default()
            .entry(plate.clone())
            .or_default();

        entries.push((mile, time));
        if let Some((p1, p2)) = should_be_ticketed(entries, road_speed_limit) {
            let ticket = ServerMessage::Ticket {
                plate,
                road: road_id,
                p1,
                p2,
            };
            tx_out.send(ticket).await.unwrap();
            //TODO can multiple tickets need to be sent? w
        }
    }
}

fn should_be_ticketed(
    entries: &mut Vec<(Mile, TimeStamp)>,
    speed_limit: Speed,
) -> Option<((Mile, TimeStamp), (Mile, TimeStamp))> {
    let n = entries.len();
    for i in 0..n {
        let (m1, t1) = entries[i];
        for j in (i + 1)..n {
            let (m2, t2) = entries[j];
            let speed = ((m1 as f64 - m2 as f64) / (t1 as f64 - t2 as f64)).abs(); //TODO needs to be in correct unit
            if speed > speed_limit as f64 {
                entries.swap_remove(j);
                entries.swap_remove(i);
                return Some(((m1, t1), (m2, t2)));
            }
        }
    }
    None
}

async fn handle_camera(
    road: RoadID,
    position: Mile,
    road_speed_limit: Speed,
    mut rx_main: Receiver<ClientMessage>,
    tx_out: Sender<ServerMessage>,
    tx_road: Sender<Sighting>,
) {
    while let Some(msg) = rx_main.recv().await {
        match msg {
            ClientMessage::PlateDetected { plate, time } => {
                tx_road
                    .send(Sighting {
                        road_id: road,
                        road_speed_limit,
                        plate,
                        mile: position,
                        time,
                    })
                    .await
                    .unwrap();
            }
            _ => {
                tx_out
                    .send(ServerMessage::Error(
                        "recived unextected message as camera".into(),
                    ))
                    .await
                    .unwrap();
            }
        }
    }
}
