#![feature(addr_parse_ascii)]
#![feature(ip_from)]

mod client;
mod common;
mod config;
mod ip_manager;
mod packet_stream;
mod server;

use anyhow::Context;
use client::Client;
use config::{load_config, Mode};
use server::Server;
use tokio::runtime::Builder;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let config = load_config(std::env::args().nth(1).context("no config file provided")?)?;
    let runtime = Builder::new_current_thread()
        .enable_io()
        .build()
        .context("could not create runtime")?;

    match config.mode {
        Mode::Client(client_config) => {
            let client = Client::try_new(client_config, config.tls)?;
            runtime.block_on(client.run())
        }
        Mode::Server(server_config) => runtime.block_on(async move {
            Server::try_new(server_config, config.tls)
                .map(|server| server.run())?
                .await
        }),
    }
}
