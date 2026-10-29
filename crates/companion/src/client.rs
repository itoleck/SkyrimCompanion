//! Background connection to the SKSE exporter. Reconnects forever and keeps
//! only the latest snapshot.

use protocol::Snapshot;
use std::io::{BufRead, BufReader};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub enum Status {
    #[default]
    Connecting,
    Connected,
    Disconnected(String),
}

#[derive(Default)]
pub struct Shared {
    pub status: Status,
    pub snapshot: Option<Snapshot>,
    pub last_update: Option<Instant>,
}

pub fn spawn(addr: String, ctx: egui::Context) -> Arc<Mutex<Shared>> {
    let shared = Arc::new(Mutex::new(Shared::default()));
    let out = shared.clone();
    std::thread::Builder::new()
        .name("skse-client".into())
        .spawn(move || loop {
            set_status(&out, Status::Connecting, &ctx);
            let err = match run(&addr, &out, &ctx) {
                Ok(()) => "connection closed".to_string(),
                Err(e) => e.to_string(),
            };
            set_status(&out, Status::Disconnected(err), &ctx);
            std::thread::sleep(Duration::from_secs(2));
        })
        .expect("spawn client thread");
    shared
}

fn set_status(shared: &Mutex<Shared>, status: Status, ctx: &egui::Context) {
    shared.lock().unwrap().status = status;
    ctx.request_repaint();
}

fn run(addr: &str, shared: &Mutex<Shared>, ctx: &egui::Context) -> std::io::Result<()> {
    let sock = addr
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| std::io::Error::other("bad address"))?;
    let stream = TcpStream::connect_timeout(&sock, Duration::from_secs(2))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    set_status(shared, Status::Connected, ctx);

    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        match protocol::decode_line(&line) {
            Ok(snapshot) => {
                let mut s = shared.lock().unwrap();
                s.snapshot = Some(snapshot);
                s.last_update = Some(Instant::now());
                drop(s);
                ctx.request_repaint();
            }
            Err(e) => eprintln!("bad snapshot: {e}"),
        }
    }
}
