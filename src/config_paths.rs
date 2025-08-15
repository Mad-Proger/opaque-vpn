use crate::{AppWindow, LineEditInfo};
use anyhow::Context;

use slint::{ModelRc, VecModel};
use std::fs;
use std::path::{Path, PathBuf};

pub fn collect_config_paths(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    if !dir.exists() {
        fs::create_dir_all(dir).context("could not create config dir")?;
    }

    for entry in fs::read_dir(dir).context("could not open config dir")? {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() || path.extension().and_then(|s| s.to_str()) != Some("toml") {
            continue;
        }
        paths.push(path);
    }

    Ok(paths)
}

pub fn paths_to_profiles(paths: &[std::path::PathBuf]) -> ModelRc<LineEditInfo> {
    let vec: Vec<LineEditInfo> = paths
        .iter()
        .map(|p| {
            let label = p
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .into();
            let text = p.to_str().unwrap_or_default().into();
            LineEditInfo { label, text }
        })
        .collect();

    ModelRc::new(VecModel::from(vec))
}

pub fn set_profiles(app: &AppWindow) {
    let config_paths = match collect_config_paths(Path::new("config")) {
        Ok(addrs) => addrs,
        Err(error) => {
            app.set_message(format!("{error:#}").into());
            Vec::new()
        }
    };

    app.set_profiles(paths_to_profiles(&config_paths));
}
