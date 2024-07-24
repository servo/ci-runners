use std::{
    env, fs,
    path::{Path, PathBuf},
};

use jane_eyre::eyre;

pub fn get_data_path(path: impl AsRef<Path>) -> eyre::Result<PathBuf> {
    let data = if let Ok(path) = env::var("SERVO_CI_MONITOR_DATA_PATH") {
        path.into()
    } else {
        PathBuf::from("./data")
    };

    fs::create_dir_all(&data)?;

    Ok(data.join(path))
}
