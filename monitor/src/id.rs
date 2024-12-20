use std::{
    fs::File,
    io::{Read, Write},
};

use jane_eyre::eyre::{self, Context};
use tracing::warn;

use crate::data::get_data_path;

/// Generates new runner ids, with persistence to disk.
pub struct IdGen {
    last: Option<usize>,
}

impl IdGen {
    pub fn new_load() -> eyre::Result<Self> {
        if let Ok(mut file) = File::open(get_data_path("last-runner-id")?) {
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
        let path = get_data_path("last-runner-id")?;
        let new_path = get_data_path("last-runner-id.new")?;
        let mut file = File::create(&new_path)?;
        file.write_all(last.to_string().as_bytes())?;
        std::fs::rename(&new_path, &path)?;

        Ok(())
    }
}
