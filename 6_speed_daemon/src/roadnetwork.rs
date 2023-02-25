use crate::messages::*;
use async_channel;
use dashmap::DashMap;
use futures::stream::select_all;
use std::collections::{HashMap, HashSet};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;

pub const CHANNEL_SIZE: usize = 10000;

#[derive(Debug)]
pub struct Sighting {
    pub road: RoadID,
    pub speed_limit: Speed,
    pub plate: Plate,
    pub position: Mile,
    pub time: TimeStamp,
}

#[derive(Default)]
pub struct RoadNetwork {
    roads: DashMap<RoadID, Road>,
}

pub struct Road {
    speed_limit: Option<Speed>,
    rx_road: async_channel::Receiver<ServerMessage>,
    tx_road: mpsc::Sender<Sighting>,
}

impl Default for Road {
    fn default() -> Self {
        let (tx_road, rx_road) = mpsc::channel(CHANNEL_SIZE);
        let (tx_road_ticket, rx_road_ticket) = async_channel::unbounded();
        tokio::spawn(async move { Self::road_ticket_handler(rx_road, tx_road_ticket).await });
        Road {
            speed_limit: None,
            rx_road: rx_road_ticket,
            tx_road,
        }
    }
}

impl Road {
    async fn road_ticket_handler(
        mut rx_road: mpsc::Receiver<Sighting>,
        tx_road_ticket: async_channel::Sender<ServerMessage>,
    ) -> Option<()> {
        let mut log = HashMap::<Plate, Vec<(Mile, TimeStamp)>>::new();
        let mut ticket_days = HashMap::<Plate, HashSet<u32>>::new();
        while let Some(Sighting {
            road: road_id,
            speed_limit: road_speed_limit,
            plate,
            position: mile,
            time,
        }) = rx_road.recv().await
        {
            let road_sightings = log.entry(plate.clone()).or_default();
            road_sightings.push((mile, time));
            let car_ticket_days = ticket_days.entry(plate.clone()).or_default();
            if let Some((p1, p2, speed)) =
                should_be_ticketed(road_sightings, road_speed_limit, car_ticket_days)
            {
                let ticket = ServerMessage::Ticket {
                    plate,
                    road: road_id,
                    p1,
                    p2,
                    speed,
                };
                let _ = tx_road_ticket.send(ticket).await;
            }
        }
        Some(())
    }
}

impl RoadNetwork {
    pub fn register_camera(&self, road: RoadID, speed_limit: Speed) -> mpsc::Sender<Sighting> {
        let mut road = self.roads.entry(road).or_default();
        road.speed_limit = Some(speed_limit);
        road.tx_road.clone()
    }

    pub fn register_dispatcher(
        &self,
        roads: Vec<RoadID>,
    ) -> impl StreamExt<Item = ServerMessage> + Unpin {
        let mut all = Vec::new();
        for road in roads {
            let road = self.roads.entry(road).or_default();
            let rx_road = road.rx_road.clone();
            all.push(rx_road);
        }
        select_all(all)
    }
}

fn should_be_ticketed(
    entries: &mut Vec<(Mile, TimeStamp)>,
    speed_limit: Speed,
    ticket_days: &mut HashSet<u32>,
) -> Option<((Mile, TimeStamp), (Mile, TimeStamp), Speed)> {
    let n = entries.len();
    for i in 0..n {
        let (m1, t1) = entries[i];
        for j in (i + 1)..n {
            let (m2, t2) = entries[j];
            let speed = 3600.0 * ((m1 as f64 - m2 as f64) / (t1 as f64 - t2 as f64)).abs();
            if speed > speed_limit as f64 {
                let d1 = t1 / 86400;
                let d2 = t2 / 86400;
                let has_been_ticketed_for_day =
                    ticket_days.contains(&d1) || ticket_days.contains(&d2);
                if !has_been_ticketed_for_day {
                    entries.swap_remove(j);
                    entries.swap_remove(i);
                    ticket_days.insert(d1);
                    ticket_days.insert(d2);
                    return Some(((m1, t1), (m2, t2), (100.0 * speed).round() as Speed));
                }
            }
        }
    }
    None
}
