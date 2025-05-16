pub mod client;
pub mod server;

use clap::{Parser, Subcommand};

use client::{Client, ClientConfig};
use server::{Server, ServerConfig};

pub fn is_disconnect(e: &std::io::Error) -> bool {
    use std::io::ErrorKind::*;
    matches!(
        e.kind(),
        BrokenPipe | ConnectionReset | UnexpectedEof | ConnectionAborted
    )
}

#[derive(Parser, Debug)]
#[clap(
    author,
    version,
    about,
    long_about = "Load and monitor KaDOS on a real Raspberry Pi"
)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Run in chainloader / serial client mode
    Client(ClientConfig),
    /// Run in GDB remote / serial server mode
    Server(ServerConfig),
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let args = Args::parse();
    match args.command {
        Command::Client(cfg) => {
            log::info!("Running as client");
            let mut client = Client::connect(&cfg).await?;
            client.send_kernel().await?;
            client.monitor().await?;
        }
        Command::Server(cfg) => {
            log::info!("Running as server");
            let server = Server::bind(&cfg).await?;
            server.serve().await?;
        }
    }

    Ok(())
}
