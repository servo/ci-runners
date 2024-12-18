use std::{
    fs,
    path::{Path, PathBuf},
};

use jane_eyre::eyre;

use crate::DOTENV;

pub fn get_data_path(path: impl AsRef<Path>) -> eyre::Result<PathBuf> {
    let data = if let Some(path) = &DOTENV.monitor_data_path {
        path.into()
    } else {
        PathBuf::from("./data")
    };

    fs::create_dir_all(&data)?;

    Ok(data.join(path))
}

pub fn get_runner_data_path(id: usize, path: impl AsRef<Path>) -> eyre::Result<PathBuf> {
    let runner_data = get_data_path(id.to_string())?;

    Ok(runner_data.join(path))
}
