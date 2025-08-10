#![feature(ip_from)]
// TODO: remove
#![allow(dead_code)]

mod client;
mod common;
mod config;
mod config_paths;
mod ip_manager;
mod packet_stream;
mod protocol;
mod routing;
mod server;

use std::{path::Path, sync::LazyLock, sync::Mutex};

use anyhow::Context;
use slint::SharedString;
use tokio::{
    runtime::{Builder, Runtime},
    sync::watch,
};

slint::include_modules!();

use crate::{
    client::Client,
    config::{Mode, load_config},
    config_paths::{PathView, collect_config_paths, paths_to_model},
};

static STOP_SENDER: LazyLock<Mutex<Option<watch::Sender<bool>>>> =
    LazyLock::new(|| Mutex::new(None));

static TOKIO_RUNTIME: LazyLock<Runtime> = LazyLock::new(|| {
    Builder::new_multi_thread()
        .worker_threads(1)
        .enable_io()
        .enable_time()
        .build()
        .expect("could not create runtime")
});

fn get_client(app: &AppWindow) -> anyhow::Result<Client> {
    let config_path = app.get_selection_from_file();
    let config = load_config(config_path).context("failed to load config")?;

    match config.mode {
        Mode::Client(client_config) => {
            Client::try_new(client_config, config.tls).context("failed to build client")
        }
        _ => Err(anyhow::anyhow!("config is not in client mode")),
    }
}

fn connect_handler(app_weak: &slint::Weak<AppWindow>, client: Client) {
    TOKIO_RUNTIME.spawn({
        let app_weak = app_weak.clone();
        async move {
            if let Err(e) = client.run().await {
                let msg = format!("{:#}", e);
                let _ = slint::invoke_from_event_loop(move || {
                    if let Some(app) = app_weak.upgrade() {
                        app.set_error_message(msg.into());
                        app.set_status_connect(SharedString::from("Disconnected"));
                    } else {
                        eprintln!("failed to upgrade link in connect_handler");
                    }
                });
            }
        }
    });
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let app = AppWindow::new()?;
    app.set_window_title(SharedString::from("Opaque VPN"));
    let app_weak = app.as_weak();

    let config_paths = match collect_config_paths(Path::new("config")) {
        Ok(addrs) => addrs,
        Err(error) => {
            app.set_error_message(format!("{error:#}").into());
            Vec::new()
        }
    };

    app.set_labels_profiles(paths_to_model(&config_paths, PathView::Stem));
    app.set_paths_profiles(paths_to_model(&config_paths, PathView::FullPath));

    app.on_open_file(move || {
        rfd::FileDialog::new()
            .pick_file()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
            .into()
    });

    let app_weak_connect = app_weak.clone();
    app.on_connect(move || {
        if let Some(app) = app_weak_connect.upgrade() {
            app.set_status_connect(SharedString::from("Connecting..."));

            let client = match get_client(&app) {
                Ok(c) => c,
                Err(error) => {
                    app.set_error_message(format!("{error:#}").into());
                    app.set_status_connect(SharedString::from("Disconnected"));
                    return;
                }
            };

            *STOP_SENDER.lock().unwrap() = Some(client.stop_sender());
            connect_handler(&app_weak_connect, client);
            app.set_status_connect(SharedString::from("Connected"));
        }
    });

    app.on_disconnect(move || {
        if let Some(sender) = STOP_SENDER.lock().unwrap().take() {
            if let Err(e) = sender.send(true) {
                if let Some(app) = app_weak.upgrade() {
                    app.set_error_message(format!("{e:#}").into());
                } else {
                    eprintln!("failed to upgrade link in disconnect");
                }
            } else {
                if let Some(app) = app_weak.upgrade() {
                    app.set_status_connect(SharedString::from("Disconnected"));
                } else {
                    eprintln!(
                        "[UI ERROR] Could not upgrade AppWindow to set status to Disconnected"
                    );
                }
            }
        } else {
            if let Some(app) = app_weak.upgrade() {
                app.set_error_message("no active connection to stop".into());
            } else {
                eprintln!("failed to upgrade link in disconnect");
            }
        }
    });

    app.run()?;
    Ok(())
}
