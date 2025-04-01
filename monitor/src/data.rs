use std::{
    fs::{self, create_dir_all, read_dir, rename, File},
    path::{Path, PathBuf},
};

use jane_eyre::eyre;
use tracing::info;

use crate::{profile::Profile, DOTENV, LIB_MONITOR_DIR};

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

pub fn get_runner_data_path<'p>(
    id: usize,
    path: impl Into<Option<&'p Path>>,
) -> eyre::Result<PathBuf> {
    let runner_data = get_data_path(Path::new("runners"))?.join(id.to_string());

    Ok(match path.into() {
        Some(path) => runner_data.join(path),
        None => runner_data,
    })
}

pub fn get_profile_data_path<'p>(
    key: &str,
    path: impl Into<Option<&'p Path>>,
) -> eyre::Result<PathBuf> {
    let profile_data = get_data_path(Path::new("profiles"))?.join(key);

    Ok(match path.into() {
        Some(path) => profile_data.join(path),
        None => profile_data,
    })
}

pub fn get_profile_configuration_path<'p>(
    profile: &Profile,
    path: impl Into<Option<&'p Path>>,
) -> PathBuf {
    let profile_data = Path::new(&*LIB_MONITOR_DIR).join(&profile.configuration_name);

    match path.into() {
        Some(path) => profile_data.join(path),
        None => profile_data,
    }
}

#[tracing::instrument]
pub fn run_migrations() -> eyre::Result<()> {
    let migrations_dir = get_data_path(Path::new("migrations"))?;
    create_dir_all(&migrations_dir)?;

    for version in 1.. {
        let marker_path = migrations_dir.join(version.to_string());
        if marker_path.try_exists()? {
            continue;
        }
        match version {
            1 => {
                info!("Moving per-runner data to runners subdirectory");
                let runners_dir = get_data_path(Path::new("runners"))?;
                create_dir_all(&runners_dir)?;
                for entry in read_dir(get_data_path(None)?)? {
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
