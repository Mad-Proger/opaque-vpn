#![feature(ip_from)]

mod client;
mod common;
mod config;
mod config_paths;
mod ip_manager;
mod packet_stream;
mod protocol;
mod routing;
mod server;

use anyhow::Context;
use log::error;
use std::path::Path;
use tokio::runtime::Builder;

use slint::SharedString;
slint::include_modules!();

use crate::{
    client::Client,
    config::{load_config, Mode},
    config_paths::{collect_config_paths, paths_to_model, PathView},
};

fn popup_error(app: &AppWindow, error: anyhow::Error) {
    let dlg = ErrorWindow::new().expect("error initializing error window");
    dlg.set_error_message(format!("{:#}", error).into());
    app.set_freeze(true);

    let app_weak = app.as_weak();
    let dlg_weak = dlg.as_weak();

    dlg.on_clicked(move || {
        if let Some(app) = app_weak.upgrade() {
            app.set_freeze(false);
        }
        if let Some(dlg) = dlg_weak.upgrade() {
            dlg.window().hide().expect("cannot stop erroring");
        }
    });

    dlg.window().show().expect("error erroring");
}

fn connect_handler(app: &AppWindow) -> anyhow::Result<()> {
    let config_path = app.get_selection_profile();
    let config = load_config(config_path).context("failed to load config")?;

    let runtime = Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .context("could not create runtime")?;

    match config.mode {
        Mode::Client(client_config) => {
            let client =
                Client::try_new(client_config, config.tls).context("failed to build client")?;
            let stop_sender = client.stop_sender();
            ctrlc::set_handler(move || {
                if let Err(err) = stop_sender.send(true) {
                    error!("could not stop: {err}");
                }
            })
            .context("could not set Ctrl-C handler")?;

            runtime
                .block_on(client.run())
                .context("client run failed")?;
            Ok(())
        }
        _ => Err(anyhow::anyhow!("Only client mode is supported")),
    }
}

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let app = AppWindow::new()?;
    app.set_window_title(SharedString::from("Opaque VPN"));
    let app_weak = app.as_weak();

    let config_paths = match collect_config_paths(Path::new("config")) {
        Ok(addrs) => addrs,
        Err(e) => {
            popup_error(&app, e);
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

    app.on_connect({
        let app_weak = app_weak.clone();
        move || {
            if let Some(app) = app_weak.upgrade() {
                if let Err(e) = connect_handler(&app) {
                    popup_error(&app, e);
                }
            }
        }
    });

    app.run()?;
    Ok(())
}
