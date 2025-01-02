use std::{
    fs::{self, create_dir_all, read_dir, rename, File},
    path::{Path, PathBuf},
};

use jane_eyre::eyre;
use tracing::info;

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
    let runner_data = get_data_path("runners")?.join(id.to_string());

    Ok(runner_data.join(path))
}

pub fn get_profile_data_path(key: &str, path: impl AsRef<Path>) -> eyre::Result<PathBuf> {
    let profile_data = get_data_path("profiles")?.join(key);

    Ok(profile_data.join(path))
}

#[tracing::instrument]
pub fn run_migrations() -> eyre::Result<()> {
    let migrations_dir = get_data_path("migrations")?;
    create_dir_all(&migrations_dir)?;

    for version in 1.. {
        let marker_path = migrations_dir.join(version.to_string());
        if marker_path.try_exists()? {
            continue;
        }
        match version {
            1 => {
                info!("Moving per-runner data to runners subdirectory");
                let runners_dir = get_data_path("runners")?;
                create_dir_all(&runners_dir)?;
                for entry in read_dir(get_data_path(".")?)? {
                    let entry = entry?;
                    // Move entries that parse as a runner id (usize)
                    if entry
                        .file_name()
                        .to_str()
                        .is_some_and(|n| n.parse::<usize>().is_ok())
                    {
                        rename(entry.path(), runners_dir.join(entry.file_name()))?;
                    }
                }
            }
            _ => break,
        }
        File::create(marker_path)?;
    }

    Ok(())
}
