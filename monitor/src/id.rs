use std::{
    fs::File,
    io::{Read, Write},
    path::Path,
};

use atomic_write_file::AtomicWriteFile;
use jane_eyre::eyre::{self, Context};
use settings::data::get_data_path;
use tracing::warn;

/// Generates new runner ids, with persistence to disk.
pub struct IdGen {
    last: Option<usize>,
}

impl IdGen {
    pub fn new_load() -> eyre::Result<Self> {
        if let Ok(mut file) = File::open(get_data_path(Path::new("last-runner-id"))?) {
            let mut last = String::default();
            file.read_to_string(&mut last)
                .wrap_err("Failed to read last runner id")?;
            let last = last.parse().wrap_err("Failed to parse last runner id")?;

            Ok(Self { last: Some(last) })
        } else {
            Ok(Self::new_empty())
        }
    }

    pub fn new_empty() -> Self {
        Self { last: None }
    }

    /// Returns a new runner id, then write it to a file.
    ///
    /// If writing fails, log a warning.
    pub fn next(&mut self) -> usize {
        let last = self.last.map_or(0, |id| id + 1);
        self.last = Some(last);
        if let Err(error) = self.write_last(last) {
            warn!(?error, "Failed to write last-runner-id: {error}");
        }

        last
    }

    fn write_last(&self, last: usize) -> eyre::Result<()> {
        let path = get_data_path(Path::new("last-runner-id"))?;
        let mut file = AtomicWriteFile::open(&path)?;
        file.write_all(last.to_string().as_bytes())?;
        file.commit()?;

        Ok(())
    }
}
