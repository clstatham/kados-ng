pub mod client;
pub mod server;

use anyhow::Result;
use clap::{Parser, Subcommand};

use client::{Client, ClientConfig};
use server::{Server, ServerConfig};

#[derive(Parser, Debug)]
#[clap(author, version, about)]
struct Args {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    Client(ClientConfig),
    Server(ServerConfig),
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
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
