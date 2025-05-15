use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::Result;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf},
    net::{TcpListener, tcp::OwnedWriteHalf},
    sync::Mutex,
};
use tokio_serial::SerialStream;

#[derive(Debug, clap::Args)]
pub struct ServerConfig {
    #[clap(default_value_t = String::from("/dev/ttyUSB0"))]
    device: String,
    #[clap(default_value_t = 921600)]
    baud: u32,
    #[clap(long, default_value_t = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1234)))]
    gdb_addr: SocketAddr,
    #[clap(long, default_value_t = SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 1235)))]
    monitor_addr: SocketAddr,
    #[clap(long, default_value_t = 4096)]
    chunk_size: usize,
}

pub struct Server {
    serial_tx: Mutex<WriteHalf<SerialStream>>,
    serial_rx: Mutex<ReadHalf<SerialStream>>,
    gdb_socket: TcpListener,
    monitor_socket: TcpListener,
    gdb_client: Mutex<Option<OwnedWriteHalf>>,
    monitor_client: Mutex<Option<OwnedWriteHalf>>,
    in_transfer_mode: AtomicBool,
    chunk_size: usize,
}

impl Server {
    pub async fn bind(config: &ServerConfig) -> Result<Self> {
        let serial_port = SerialStream::open(&tokio_serial::new(&config.device, config.baud))?;
        let (rx, tx) = tokio::io::split(serial_port);
        let gdb_socket = TcpListener::bind(config.gdb_addr).await?;
        let monitor_socket = TcpListener::bind(config.monitor_addr).await?;

        Ok(Self {
            serial_tx: Mutex::new(tx),
            serial_rx: Mutex::new(rx),
            gdb_socket,
            monitor_socket,
            gdb_client: Mutex::new(None),
            monitor_client: Mutex::new(None),
            in_transfer_mode: AtomicBool::new(false),
            chunk_size: config.chunk_size,
        })
    }

    pub async fn serve(&self) -> Result<()> {
        tokio::try_join!(self.serve_serial(), self.serve_gdb(), self.serve_monitor())?;

        Ok(())
    }

    async fn read_and_forward_to_monitor(&self, buf: &mut [u8]) -> Result<usize> {
        if let Some(mon) = self.monitor_client.lock().await.as_mut() {
            let size = self.serial_rx.lock().await.read_exact(buf).await?;
            mon.write_all(&buf[..size]).await?;
            mon.flush().await?;
            Ok(size)
        } else {
            Ok(0)
        }
    }

    async fn forward_to_gdb(&self, buf: &[u8]) -> Result<()> {
        if let Some(gdb) = self.gdb_client.lock().await.as_mut() {
            gdb.write_all(buf).await?;
            gdb.flush().await?;
        }
        Ok(())
    }

    async fn serve_serial(&self) -> Result<()> {
        loop {
            let mut buf = [0u8; 1];
            let size = self.read_and_forward_to_monitor(&mut buf).await?;
            if size == 0 {
                continue;
            }
            if self.in_transfer_mode.load(Ordering::SeqCst) {
                continue;
            }

            let [data] = buf;
            if data == b'+' || data == b'-' {
                self.forward_to_gdb(&[data]).await?;
            } else if data == b'$' {
                let mut packet = vec![data];
                loop {
                    self.read_and_forward_to_monitor(&mut buf).await?;
                    let [byte] = buf;
                    packet.push(byte);
                    if byte == b'#' {
                        let mut buf = [0u8; 2];
                        self.read_and_forward_to_monitor(&mut buf).await?;
                        packet.extend(buf);
                        break;
                    }
                }
                if let Ok(s) = str::from_utf8(&packet) {
                    log::info!("[serial -> gdb] {s}");
                }
                self.forward_to_gdb(&packet).await?;
            }
        }
    }

    async fn serve_gdb(&self) -> Result<()> {
        let (conn, addr) = self.gdb_socket.accept().await?;
        log::info!("Accepted GDB connection from {addr}");
        let (mut conn_rx, conn_tx) = conn.into_split();
        *self.gdb_client.lock().await = Some(conn_tx);
        let mut buf = vec![0u8; self.chunk_size];
        loop {
            let size = conn_rx.read(&mut buf).await?;
            if size > 0 {
                let data = &buf[..size];
                if let Ok(s) = str::from_utf8(data) {
                    log::info!("[gdb -> serial] {}", s.trim());
                }
                let mut port = self.serial_tx.lock().await;
                port.write_all(data).await?;
                port.flush().await?;
            }
        }
    }

    async fn serve_monitor(&self) -> Result<()> {
        let (conn, addr) = self.monitor_socket.accept().await?;
        log::info!("Accepted Monitor connection from {addr}");
        let (mut conn_rx, conn_tx) = conn.into_split();
        *self.monitor_client.lock().await = Some(conn_tx);
        let mut buf = vec![0u8; self.chunk_size];
        loop {
            let size = conn_rx.read(&mut buf).await?;
            let data = &buf[..size];
            if data.starts_with(b"BEGINBEGINBEGINBEGIN") {
                log::info!("[mon] Begin kernel transfer");
                self.in_transfer_mode.store(true, Ordering::SeqCst);
            } else if data.starts_with(b"ENDENDENDENDENDENDEND") {
                log::info!("[mon] End kernel transfer");
                self.in_transfer_mode.store(false, Ordering::SeqCst);
            } else if size > 0 {
                // if let Ok(s) = str::from_utf8(data) {
                //     if !s.trim().is_empty() {
                //         log::info!("[mon -> serial] {}", s.trim());
                //     }
                // }
                let mut port = self.serial_tx.lock().await;
                port.write_all(data).await?;
                port.flush().await?;
            }
        }
    }
}
