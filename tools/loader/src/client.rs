use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
};

use anyhow::{Result, bail};
use indicatif::{ProgressBar, ProgressStyle};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};
use xmas_elf::{ElfFile, sections::SectionData, symbol_table::Entry};

#[derive(Debug, clap::Args)]
pub struct ClientConfig {
    kernel_path: PathBuf,
    #[clap(long)]
    symbol_path: Option<PathBuf>,
    #[clap(long, default_value_t = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 1235))]
    addr: SocketAddr,
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
    pub async fn connect(config: &ClientConfig) -> Result<Self> {
        let kernel = tokio::fs::read(&config.kernel_path).await?;
        let symbols = if let Some(symbol_path) = &config.symbol_path {
            Some(tokio::fs::read(symbol_path).await?)
        } else {
            None
        };

        let conn = TcpStream::connect(config.addr).await?;

        Ok(Self {
            kernel,
            symbols,
            conn,
            chunk_size: config.chunk_size,
        })
    }

    pub async fn send_kernel(&mut self) -> Result<()> {
        log::info!("Power cycle your Pi now!");
        let mut num_breaks = 0;
        while num_breaks < 3 {
            let c = self.conn.read_u8().await?;
            if c == b'\x03' {
                num_breaks += 1;
            } else {
                num_breaks = 0;
            }
        }
        log::info!("Sending kernel size ({:#x} bytes)", self.kernel.len());
        self.conn
            .write_all(&(self.kernel.len() as u32).to_le_bytes())
            .await?;
        self.conn.flush().await?;
        log::info!("Awaiting size response");
        let mut ok = [0u8; 2];
        self.conn.read_exact(&mut ok).await?;
        if &ok != b"OK" {
            bail!("Size error");
        }

        log::info!("Sending kernel...");
        self.conn.write_all(b"BEGINBEGINBEGINBEGIN").await?;
        self.conn.flush().await?;

        let it = self.kernel.chunks(self.chunk_size);
        let pbar =
            ProgressBar::new(self.kernel.len() as u64).with_style(ProgressStyle::default_bar());
        for chunk in it {
            self.conn.write_all(chunk).await?;
            self.conn.flush().await?;
            let mut echo = vec![0u8; chunk.len()];
            self.conn.read_exact(&mut echo).await?;
            if echo != chunk {
                let first_diff = chunk.iter().zip(&echo).position(|(a, b)| a != b).unwrap();
                bail!(
                    "Kernel transfer error: echo[{}] = {} != {}",
                    first_diff,
                    echo[first_diff],
                    chunk[first_diff],
                );
            }
            pbar.inc(chunk.len() as u64);
        }
        pbar.finish();
        let mut ty = [0u8; 4];
        self.conn.read_exact(&mut ty).await?;
        if &ty != b"TY:)" {
            bail!("Error in loader acknowledgement");
        }

        log::info!("Kernel sent!");
        self.conn.write_all(b"ENDENDENDENDENDENDEND").await?;
        self.conn.flush().await?;

        Ok(())
    }

    pub async fn monitor(&mut self) -> Result<()> {
        // ignore that this is unused, it's still TODO
        let symbols = self
            .symbols
            .as_ref()
            .map(|b| ElfFile::new(b))
            .transpose()
            .map_err(|e| {
                log::error!("Error parsing symbol file: {e}");
                anyhow::Error::msg(e)
            })?;

        let (mut rx, mut tx) = tokio::io::split(&mut self.conn);
        let mut reader = BufReader::new(&mut rx);
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).await?;
            let trimmed = line.trim_start();
            if trimmed.starts_with('+') || trimmed.starts_with('-') || trimmed.starts_with('$') {
                continue;
            }
            if trimmed.starts_with("[sym?]") {
                if let Some(symbols) = symbols.as_ref() {
                    let addr = trimmed.strip_prefix("[sym?]").unwrap().parse::<u64>()?;
                    if let Some(symtab) = symbols.find_section_by_name(".symtab") {
                        let Ok(SectionData::SymbolTable64(syms)) = symtab.get_data(symbols) else {
                            tx.write_all(b"unknown\n").await?;
                            continue;
                        };
                        for entry in syms {
                            if (entry.value()..entry.value() + entry.size()).contains(&addr) {
                                let Ok(name) = entry.get_name(symbols) else {
                                    tx.write_all(b"unknown\n").await?;
                                    tx.flush().await?;
                                    continue;
                                };
                                tx.write_all(name.as_bytes()).await?;
                                tx.write_all(b"\n").await?;
                                tx.flush().await?;
                                break;
                            }
                        }
                        tx.write_all(b"unknown\n").await?;
                        tx.flush().await?;
                        continue;
                    } else {
                        tx.write_all(b"unknown\n").await?;
                        tx.flush().await?;
                        continue;
                    }
                } else {
                    tx.write_all(b"unknown\n").await?;
                    tx.flush().await?;
                    continue;
                }
            }
            tokio::io::stdout().write_all(line.as_bytes()).await?;
            tokio::io::stdout().flush().await?;
        }
    }
}
