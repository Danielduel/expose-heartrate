extern crate futures;
extern crate tokio;
extern crate websocket;

use arctic::async_trait;
use futures::sink::Send;
use std::collections::HashMap;
use std::fmt::Debug;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{thread, usize};
use tokio::runtime::Runtime;
use websocket::ws;

use websocket::message::{Message, OwnedMessage};
use websocket::server::{InvalidConnection, NoTlsAcceptor, WsServer};
use websocket::sync::Server;

use futures::{future, Future, Sink, Stream};

static HEARTRATE: AtomicUsize = AtomicUsize::new(0);

struct Handler;

#[async_trait]
impl arctic::EventHandler for Handler {
    async fn battery_update(&self, battery_level: u8) {
        // println!("Battery: {}", battery_level);
    }

    async fn heartrate_update(&self, _ctx: &arctic::PolarSensor, heartrate: u16) {
        HEARTRATE.swap(heartrate as usize, Ordering::Relaxed);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    HEARTRATE.swap(0 as usize, Ordering::Relaxed);
    // let executor = tokio::runtime::Builder::new_multi_thread().build().unwrap();
    let ws = Server::bind("localhost:2137").unwrap();

    let websocket_task = tokio::spawn(async move {
        println!("Start websocket");
        websocket_server_tick(ws);
        println!("End websocket");
    });

    let sensor = setup_polar().await.unwrap();
    let sensor_task = tokio::spawn(async move {
        println!("Sensor event_loop start");
        sensor.event_loop().await.unwrap();
        println!("Sensor event_loop stop");
    });

    websocket_task.await.unwrap();
    sensor_task.await.unwrap();

    Ok(())
}

async fn construct_sensor() -> Result<arctic::PolarSensor, ()> {
    let mut polar_o: Option<arctic::PolarSensor> = None;
    while polar_o.is_none() {
        match arctic::PolarSensor::new("9333D32A".to_string()).await {
            Ok(sensor) => {
                polar_o = Some(sensor);
                println!("Sensor constructed");
            }
            Err(x) => {
                println!("Retrying sensor construction");
            }
        }
    }
    return Ok(polar_o.unwrap());
}

async fn connect_sensor() -> Result<arctic::PolarSensor, ()> {
    let mut polar = construct_sensor().await.unwrap();

    while !polar.is_connected().await {
        println!("Trying to connect...");
        match polar.connect().await {
            Err(arctic::Error::NoBleAdaptor) => {
                println!("No bluetooth adapter found");
            }
            Err(why) => {
                println!("Could not connect: {:?}", why);
                polar = construct_sensor().await.unwrap();
                println!("Retrying");
            }
            _ => {}
        }
    }
    if let Err(why) = polar.subscribe(arctic::NotifyStream::Battery).await {
        println!("Could not subscribe to battery notifications: {:?}", why)
    }
    if let Err(why) = polar.subscribe(arctic::NotifyStream::HeartRate).await {
        println!("Could not subscribe to heart rate notifications: {:?}", why)
    }
    polar.event_handler(Handler);

    return Ok(polar);
}

async fn setup_polar() -> Result<arctic::PolarSensor, ()> {
    println!("Searching for sensor");

    return connect_sensor().await;
}

fn websocket_server_tick(ws: WsServer<NoTlsAcceptor, TcpListener>) {
    println!("Listening");
    for request in ws.filter_map(Result::ok) {
        // Spawn a new thread for each connection.
        thread::spawn(|| {
            if !request.protocols().contains(&"rust-websocket".to_string()) {
                request.reject().unwrap();
                return;
            }

            let mut client = request.use_protocol("rust-websocket").accept().unwrap();

            let ip = client.peer_addr().unwrap();

            println!("Connection from {}", ip);

            let message = OwnedMessage::Text("Hello".to_string());
            client.send_message(&message).unwrap();

            let (mut receiver, mut sender) = client.split().unwrap();

            for message in receiver.incoming_messages() {
                let message = message.unwrap();

                match message {
                    OwnedMessage::Text(msg) => {
                        if msg == "bpm" {
                            let hr = HEARTRATE.load(Ordering::Acquire);
                            let message = OwnedMessage::Text(hr.to_string());
                            sender.send_message(&message).unwrap();
                        }
                    }
                    OwnedMessage::Close(_) => {
                        let message = OwnedMessage::Close(None);
                        sender.send_message(&message).unwrap();
                        println!("Client {} disconnected", ip);
                        return;
                    }
                    OwnedMessage::Ping(ping) => {
                        let message = OwnedMessage::Pong(ping);
                        sender.send_message(&message).unwrap();
                    }
                    _ => sender.send_message(&message).unwrap(),
                }
            }
        });
    }
}
