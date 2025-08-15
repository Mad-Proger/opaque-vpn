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
use log::error;
use slint::SharedString;
use tokio::{
    runtime::{Builder, Runtime},
    sync::watch,
};

slint::include_modules!();

use crate::config_paths::set_profiles;
use crate::{
    client::Client,
    config::{Mode, load_config},
    config_paths::{collect_config_paths, paths_to_profiles},
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
    let config_path = app.get_from_file().text;
    let config = load_config(config_path).context("failed to load config")?;

    match config.mode {
        Mode::Client(client_config) => {
            Client::try_new(client_config, config.tls).context("failed to build client")
        }
        _ => Err(anyhow::anyhow!("config is not in client mode")),
    }
}

fn browse_handler(app_weak: slint::Weak<AppWindow>) {
    TOKIO_RUNTIME.spawn(async move {
        let file_path = rfd::FileDialog::new()
            .pick_file()
            .map(|p| p.display().to_string());

        let _ = slint::invoke_from_event_loop(move || match app_weak.upgrade() {
            Some(app) => {
                if let Some(p) = file_path {
                    app.set_from_file(LineEditInfo {
                        text: SharedString::from(p),
                        ..app.get_from_file()
                    })
                }
            }
            None => {
                error!("app downgraded before setting selected file");
            }
        });
    });
}

fn connect_handler(app_weak: slint::Weak<AppWindow>, client: Client) {
    TOKIO_RUNTIME.spawn(async move {
        if let Err(e) = client.run().await {
            let _ = slint::invoke_from_event_loop(move || match app_weak.upgrade() {
                Some(app) => {
                    app.set_message(format!("{:#}", e).into());
                    app.set_status(SharedString::from("Disconnected"));
                }
                None => {
                    error!("failed to upgrade link in connect_handler");
                }
            });
        }
    });
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let app = AppWindow::new()?;
    app.set_window_title(SharedString::from("Opaque VPN"));
    app.set_status(SharedString::from("Disconnected"));
    let app_weak = app.as_weak();

    set_profiles(&app);

    let app_weak_browse = app_weak.clone();
    app.on_browse(move || {
        browse_handler(app_weak_browse.clone());
    });

    let app_weak_reload = app_weak.clone();
    app.on_reload(move || match app_weak_reload.upgrade() {
        Some(app) => {
            set_profiles(&app);
        }
        None => {
            error!("failed to upgrade link in disconnect");
        }
    });

    let app_weak_connect = app_weak.clone();
    app.on_connect(move || {
        if let Some(app) = app_weak_connect.upgrade() {
            app.set_status(SharedString::from("Connecting..."));

            let client = match get_client(&app) {
                Ok(c) => c,
                Err(error) => {
                    app.set_message(format!("{error:#}").into());
                    app.set_status(SharedString::from("Disconnected"));
                    return;
                }
            };

            *STOP_SENDER.lock().unwrap() = Some(client.stop_sender());
            connect_handler(app_weak_connect.clone(), client);
            app.set_status(SharedString::from("Connected"));
        }
    });

    let app_weak_disconnect = app_weak.clone();
    app.on_disconnect(move || match app_weak_disconnect.upgrade() {
        Some(app) => {
            match STOP_SENDER
                .lock()
                .unwrap()
                .take()
                .context("no active connection to stop")
                .and_then(|sender| Ok(sender.send(true)?))
            {
                Ok(()) => app.set_status("Disconnected".into()),
                Err(e) => app.set_message(format!("{e:#}").into()),
            }
        }
        None => {
            error!("failed to upgrade link in disconnect");
        }
    });

    app.run()?;
    Ok(())
}
