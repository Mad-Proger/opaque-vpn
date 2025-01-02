#![feature(addr_parse_ascii)]

mod client;
mod config;
mod ip_manager;
mod server;

use anyhow::Context;
use client::run_client;
use config::{load_config, Mode};
use server::run_server;
use tokio::runtime::Builder;

fn main() -> anyhow::Result<()> {
    let config = load_config(std::env::args().nth(1).context("no config file provided")?)?;

    let runtime = Builder::new_current_thread()
        .build()
        .context("could not create runtime")?;

    match config.mode {
        Mode::Client(client_config) => {
            runtime.block_on(async move { run_client(client_config, config.tls).await })
        }
        Mode::Server(server_config) => {
            runtime.block_on(async move { run_server(server_config, config.tls).await })
        }
    }
}
