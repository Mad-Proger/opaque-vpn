#![feature(ip_from)]

mod client;
mod common;
mod config;
mod ip_manager;
mod packet_stream;
mod protocol;
mod routing;
mod server;

use anyhow::Context;
use log::error;
use tokio::runtime::Builder;

use slint::SharedString;
slint::include_modules!();

use crate::{
    client::Client,
    config::{Mode, config_from_app, load_config},
    server::Server,
};

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let app = AppWindow::new()?;
    let weak = app.as_weak();

    app.set_window_title(SharedString::from("Opaque VPN"));

    app.on_connect(move || {
        println!("Connection established!");
        let app: AppWindow = weak.upgrade().unwrap();

        // let config_path = std::env::args()
        //     .nth(1)
        //     .context("no config file provided")
        //     .unwrap();
        // let config = load_config(config_path).unwrap();

        let config = match config_from_app(&app) {
            Ok(c) => c,
            Err(err) => {
                error!("failed to build config from UI: {err}");
                return;
            }
        };

        config.print_all();
        let runtime = Builder::new_current_thread()
            .enable_io()
            .build()
            .context("could not create runtime")
            .unwrap();

        match config.mode {
            Mode::Client(client_config) => {
                let client = Client::try_new(client_config, config.tls).unwrap();
                let stop_sender = client.stop_sender();
                ctrlc::set_handler(move || {
                    if let Err(err) = stop_sender.send(true) {
                        error!("could not stop: {err}");
                    }
                })
                .context("could not set Ctrl-C handler")
                .unwrap();

                runtime.block_on(client.run()).unwrap();
            }
            Mode::Server(server_config) => runtime
                .block_on(async move {
                    Server::try_new(server_config, config.tls)
                        .map(|server| server.run())?
                        .await
                })
                .unwrap(),
        }
    });

    app.run()?;

    Ok(())
}
