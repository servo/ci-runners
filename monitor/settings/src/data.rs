use std::{
    fs::{self},
    path::{Path, PathBuf},
};

use jane_eyre::eyre;

use crate::DOTENV;

pub fn get_data_path<'p>(path: impl Into<Option<&'p Path>>) -> eyre::Result<PathBuf> {
    let data = if let Some(path) = &DOTENV.monitor_data_path {
        path.into()
    } else {
        PathBuf::from("./data")
    };

    fs::create_dir_all(&data)?;

    Ok(match path.into() {
        Some(path) => data.join(path),
        None => data,
    })
}
