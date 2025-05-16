use std::{
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use indicatif::{ProgressBar, ProgressStyle};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpStream, tcp::WriteHalf},
};
use xmas_elf::{ElfFile, sections::SectionData, symbol_table::Entry};

#[derive(Debug, clap::Args)]
pub struct ClientConfig {
    /// Path to the kernel binary to send over serial
    kernel_path: PathBuf,
    /// Optional path to the kernel debug symbol file
    #[clap(long)]
    symbol_path: Option<PathBuf>,
    /// Address to connect to
    #[clap(long, default_value_t = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1235))]
    addr: SocketAddr,
    /// Chunk size for kernel transfer
    #[clap(long, default_value_t = 4096)]
    chunk_size: usize,
}

pub struct Client {
    kernel: Vec<u8>,
    symbols: Option<Vec<u8>>,
    conn: TcpStream,
    chunk_size: usize,
}

impl Client {
    pub async fn connect(config: &ClientConfig) -> io::Result<Self> {
        let kernel = tokio::fs::read(&config.kernel_path).await?;
        let symbols = if let Some(symbol_path) = &config.symbol_path {
            Some(tokio::fs::read(symbol_path).await?)
        } else {
            None
        };

        let conn = TcpStream::connect(config.addr).await?;
        conn.set_nodelay(true)?;

        Ok(Self {
            kernel,
            symbols,
            conn,
            chunk_size: config.chunk_size,
        })
    }

    pub async fn send_kernel(&mut self) -> io::Result<()> {
        log::info!("Sending kernel to server...");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received Ctrl+C, exiting...");
                self.conn.shutdown().await?;
            }
            res = self.send_kernel_inner() => {
                if let Err(e) = res {
                    log::error!("Error sending kernel: {e}");
                    self.conn.shutdown().await?;
                }
            }
        }
        Ok(())
    }

    async fn send_kernel_inner(&mut self) -> io::Result<()> {
        let (mut reader, mut writer) = self.conn.split();

        log::info!("Power cycle your Pi now!");
        let mut num_breaks = 0;
        while num_breaks < 3 {
            let c = reader.read_u8().await?;
            if c == b'\x03' {
                num_breaks += 1;
            } else {
                num_breaks = 0;
            }
        }

        log::info!("Sending kernel size ({:#x} bytes)", self.kernel.len());
        writer
            .write_all(&(self.kernel.len() as u32).to_le_bytes())
            .await?;

        let mut ok = [0u8; 2];
        reader.read_exact(&mut ok).await?;
        if &ok != b"OK" {
            return Err(io::Error::other("Error in kernel transfer"));
        }

        log::info!("Sending kernel...");

        let it = self.kernel.chunks(self.chunk_size);
        let pbar = ProgressBar::new(self.kernel.len() as u64).with_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}/{duration_precise}] {wide_bar} {bytes}/{total_bytes} ({bytes_per_sec})")
                .unwrap(),
        );
        let mut echo = vec![0u8; self.chunk_size];
        for chunk in it {
            writer.write_all(chunk).await?;

            let current_chunk_size = chunk.len();
            reader.read_exact(&mut echo[..current_chunk_size]).await?;
            if &echo[..current_chunk_size] != chunk {
                return Err(io::Error::other("Error in kernel transfer"));
            }
            pbar.inc(current_chunk_size as u64);
        }
        pbar.finish();
        drop(echo);
        let mut ty = [0u8; 4];
        reader.read_exact(&mut ty).await?;
        if &ty != b"TY:)" {
            return Err(io::Error::other("Error in kernel transfer"));
        }

        log::info!("Kernel sent!");

        Ok(())
    }

    pub async fn monitor(&mut self) -> io::Result<()> {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                log::info!("Received Ctrl+C, exiting...");
            }
            res = self.monitor_inner() => {
                if let Err(e) = res {
                    log::error!("Error in monitor: {e}");
                }
            }
        }
        self.conn.shutdown().await.ok();
        Ok(())
    }

    async fn monitor_inner(&mut self) -> io::Result<()> {
        let symbols = self
            .symbols
            .as_ref()
            .map(|b| ElfFile::new(b))
            .transpose()
            .map_err(|e| {
                log::error!("Error parsing symbol file: {e}");
                io::Error::new(io::ErrorKind::InvalidData, e)
            })?;

        let (mut rx, mut tx) = self.conn.split();
        let mut buf = vec![0u8; self.chunk_size];
        loop {
            let size = rx.read(&mut buf).await?;
            let data = &buf[..size];
            let is_symbol_request =
                maybe_handle_symbol_request(symbols.as_ref(), data, &mut tx).await?;
            if !is_symbol_request {
                tokio::io::stdout().write_all(data).await?;
                tokio::io::stdout().flush().await?;
            }
        }
    }
}

async fn maybe_handle_symbol_request(
    symbols: Option<&ElfFile<'_>>,
    data: &[u8],
    tx: &mut WriteHalf<'_>,
) -> io::Result<bool> {
    if data.starts_with(b"[sym?]") {
        if let Some(symbols) = symbols.as_ref() {
            let addr = String::from_utf8_lossy(data.strip_prefix(b"[sym?]").unwrap());
            if let Ok(addr) = addr.trim().parse::<u64>() {
                if let Some(name) = find_symbol(symbols, addr) {
                    tx.write_all(name).await?;
                    tx.write_all(b"\n").await?;
                } else {
                    tx.write_all(b"unknown\n").await?;
                }
            } else {
                tx.write_all(b"unknown\n").await?;
            }
        } else {
            tx.write_all(b"unknown\n").await?;
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

fn find_symbol<'a>(symbols: &ElfFile<'a>, addr: u64) -> Option<&'a [u8]> {
    if let Some(symtab) = symbols.find_section_by_name(".symtab") {
        let Ok(SectionData::SymbolTable64(syms)) = symtab.get_data(symbols) else {
            return None;
        };
        for entry in syms {
            if (entry.value()..entry.value() + entry.size()).contains(&addr) {
                let Ok(name) = entry.get_name(symbols) else {
                    return None;
                };
                return Some(name.as_bytes());
            }
        }
        None
    } else {
        None
    }
}
