use std::{
    collections::{BTreeMap, BTreeSet},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::Arc,
};

use tokio::{
    io::{self, AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::{TcpListener, tcp::OwnedWriteHalf},
    sync::{Mutex, RwLock},
    task::JoinHandle,
    time::Duration,
};
use tokio_serial::SerialStream;

use crate::is_disconnect;

#[derive(Debug, clap::Args)]
pub struct ServerConfig {
    /// Path to the serial device to connect to
    #[clap(default_value_t = String::from("/dev/ttyUSB0"))]
    device: String,
    /// Baud rate for the serial connection
    #[clap(default_value_t = 921600)]
    baud: u32,
    /// Address to bind the monitor server to
    #[clap(long, default_value_t = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1235)))]
    monitor_addr: SocketAddr,
    /// Size of serial read/write chunks
    #[clap(long, default_value_t = 16*1024)]
    chunk_size: usize,
}

pub struct SerialConnection {
    pub tx: Mutex<WriteHalf<SerialStream>>,
    pub rx: Mutex<ReadHalf<SerialStream>>,
}

impl SerialConnection {
    pub fn new(serial: SerialStream) -> Self {
        let (rx, tx) = tokio::io::split(serial);
        Self {
            tx: Mutex::new(tx),
            rx: Mutex::new(rx),
        }
    }
}

pub struct MonitorClient {
    pub tx: OwnedWriteHalf,
    pub task: JoinHandle<io::Result<()>>,
}

pub struct Server {
    serial: Arc<SerialConnection>,
    monitor_socket: TcpListener,
    monitor_clients: RwLock<BTreeMap<SocketAddr, Mutex<MonitorClient>>>,
    disconnected_clients: RwLock<BTreeSet<SocketAddr>>,
    chunk_size: usize,
}

impl Server {
    pub async fn bind(config: &ServerConfig) -> io::Result<Arc<Self>> {
        let serial_port = SerialStream::open(&tokio_serial::new(&config.device, config.baud))?;
        let monitor_socket = TcpListener::bind(config.monitor_addr).await?;
        log::info!("Listening on {}", config.monitor_addr);

        Ok(Arc::new(Self {
            serial: Arc::new(SerialConnection::new(serial_port)),
            monitor_socket,
            monitor_clients: RwLock::new(BTreeMap::new()),
            disconnected_clients: RwLock::new(BTreeSet::new()),
            chunk_size: config.chunk_size,
        }))
    }

    pub async fn serve(self: &Arc<Self>) -> io::Result<()> {
        let serial_clone = self.clone();
        let monitor_clone = self.clone();
        let reap_clone = self.clone();
        let serial_loop = tokio::spawn(serial_clone.serial_loop());
        let monitor_loop = tokio::spawn(monitor_clone.accept_monitor_connections());
        let reap_loop = tokio::spawn(reap_clone.reap_disconnected_clients());
        tokio::select! {
            res = serial_loop => {
                if let Err(e) = res {
                    log::error!("Serial loop error: {e}");
                }
            }
            res = monitor_loop => {
                if let Err(e) = res {
                    log::error!("Monitor loop error: {e}");
                }
            }
            res = reap_loop => {
                if let Err(e) = res {
                    log::error!("Reap loop error: {e}");
                }
            }
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received Ctrl+C, shutting down...");
            }
        }
        Ok(())
    }

    async fn reap_disconnected_clients(self: Arc<Self>) -> io::Result<()> {
        loop {
            tokio::time::sleep(Duration::from_millis(100)).await;
            let mut disconnected_clients = self.disconnected_clients.write().await;
            let mut monitor_clients = self.monitor_clients.write().await;

            while let Some(addr) = disconnected_clients.pop_first() {
                if let Some(client) = monitor_clients.remove(&addr) {
                    log::debug!("Removing disconnected monitor client {addr}");
                    let mut conn = client.lock().await;
                    if let Err(e) = conn.tx.shutdown().await {
                        log::error!("Error shutting down client {addr}: {e}");
                    }
                    conn.task.abort();
                } else {
                    log::debug!("Client {addr} not found in monitor clients");
                }
            }
        }
    }

    async fn schedule_disconnect(&self, addr: SocketAddr) {
        let mut disconnected_clients = self.disconnected_clients.write().await;
        if disconnected_clients.insert(addr) {
            log::debug!("Scheduled client {addr} for disconnection");
        }
    }

    async fn serial_loop(self: Arc<Self>) -> io::Result<()> {
        let mut buf = vec![0u8; self.chunk_size];
        loop {
            let n = self.serial.rx.lock().await.read(&mut buf).await?;
            if n == 0 {
                log::warn!("Serial connection closed");
                break;
            }
            let monitor_clients = self.monitor_clients.read().await;
            for (addr, client) in monitor_clients.iter() {
                let mut conn = client.lock().await;
                match conn.tx.write_all(&buf[..n]).await {
                    Ok(()) => {}
                    Err(e) => {
                        if is_disconnect(&e) {
                            log::warn!("Monitor client {addr} disconnected: {e}");
                        } else {
                            log::error!("Error writing to monitor client {addr}: {e}");
                        }
                        self.schedule_disconnect(*addr).await;
                    }
                }
            }
        }

        Ok(())
    }

    async fn accept_monitor_connections(self: Arc<Self>) -> io::Result<()> {
        loop {
            let (conn, addr) = self.monitor_socket.accept().await?;
            conn.set_nodelay(true)?;
            let (mut rx, tx) = conn.into_split();
            log::info!("Accepted monitor connection from {addr}");

            let self_clone = self.clone();
            let task = tokio::spawn(async move {
                let mut buf = vec![0u8; self_clone.chunk_size];
                loop {
                    let n = match rx.read(&mut buf).await {
                        Ok(0) => {
                            log::info!("Monitor connection from {addr} closed gracefully");
                            self_clone.schedule_disconnect(addr).await;
                            return io::Result::Ok(());
                        }
                        Ok(n) => n,
                        Err(e) if is_disconnect(&e) => {
                            log::warn!("Monitor connection from {addr} closed: {e}");
                            self_clone.schedule_disconnect(addr).await;
                            return io::Result::Ok(());
                        }
                        Err(e) => {
                            log::error!("Error reading from monitor client {addr}: {e}");
                            self_clone.schedule_disconnect(addr).await;
                            return Err(e);
                        }
                    };
                    let mut serial_tx = self_clone.serial.tx.lock().await;
                    serial_tx.write_all(&buf[..n]).await?;
                }
            });

            self.monitor_clients
                .write()
                .await
                .insert(addr, Mutex::new(MonitorClient { tx, task }));
        }
    }
}
