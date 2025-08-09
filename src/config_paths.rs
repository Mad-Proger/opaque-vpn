/*--========================================--*\
    * Author  : NTheme - All rights reserved
    * Created : 09 August 2025, 5:04 PM
    * File    : config_paths.rs
    * Project : opaque-vpn
\*--========================================--*/

use anyhow::Context;
use slint::{ModelRc, SharedString, VecModel};
use std::fs;
use std::path::{Path, PathBuf};

pub enum PathView {
    Stem,
    FullPath,
}

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

pub fn paths_to_model(paths: &[PathBuf], view: PathView) -> ModelRc<SharedString> {
    let items: Vec<SharedString> = paths
        .iter()
        .filter_map(|p| match view {
            PathView::Stem => p.file_stem()?.to_str().map(SharedString::from),
            PathView::FullPath => p.to_str().map(SharedString::from),
        })
        .collect();

    ModelRc::new(VecModel::from(items))
}
