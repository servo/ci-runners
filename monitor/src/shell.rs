use std::{
    fs::rename,
    io::{BufRead, BufReader, Read},
    os::unix::fs::symlink,
    path::Path,
    str,
};

use jane_eyre::eyre::{self, OptionExt};
use mktemp::Temp;
use reflink::reflink_or_copy;
use tracing::{info, trace, warn};

macro_rules! impl_log_output_as {
    ($name:ident, $macro:ident) => {
        /// Log the given output to tracing.
        ///
        /// Unlike cmd_lib’s built-in logging:
        /// - it handles CR-based progress output correctly, such as in `curl`, `dd`, and `rsync`
        /// - it uses `tracing` instead of `log`, so the logs show the correct target and any span context
        ///   given via `#[tracing::instrument]`
        ///
        /// This only works with `spawn_with_output!()`, and `wait_with_pipe()` only works with stdout, so
        /// if you want to log both stdout and stderr, use `spawn_with_output!(... 2>&1)`.
        pub fn $name(reader: Box<dyn Read>) {
            let mut reader = BufReader::new(reader);
            let mut buffer = vec![];
            loop {
                // Unconditionally try to read more data, since the BufReader buffer is empty
                let result = match reader.fill_buf() {
                    Ok(buffer) => buffer,
                    Err(error) => {
                        warn!(?error, "Error reading from child process");
                        break;
                    }
                };
                // Add the result onto our own buffer
                buffer.extend(result);
                // Empty the BufReader
                let read_len = result.len();
                reader.consume(read_len);

                // Log output to tracing. Take whole “lines” at every LF or CR (for progress bars etc),
                // but leave any incomplete lines in our buffer so we can try to complete them.
                while let Some(offset) = buffer.iter().position(|&b| b == b'\n' || b == b'\r') {
                    let line = &buffer[..offset];
                    let line = str::from_utf8(line).map_err(|_| line);
                    match line {
                        Ok(string) => $macro!(line = %string),
                        Err(bytes) => $macro!(?bytes),
                    }
                    buffer = buffer.split_off(offset + 1);
                }

                if read_len == 0 {
                    break;
                }
            }

            // Log any remaining incomplete line to tracing.
            if !buffer.is_empty() {
                let line = &buffer;
                let line = str::from_utf8(line).map_err(|_| line);
                match line {
                    Ok(string) => $macro!(line = %string),
                    Err(bytes) => $macro!(?bytes),
                }
            }
        }
    };
}

impl_log_output_as!(log_output_as_trace, trace);
impl_log_output_as!(log_output_as_info, info);

pub fn atomic_symlink(original: impl AsRef<Path>, link: impl AsRef<Path>) -> eyre::Result<()> {
    let link_path = link.as_ref();
    let link_parent = link_path.parent().ok_or_eyre("Link path has no parent")?;
    let link_temp = Temp::new_path_in(link_parent);
    symlink(original, &link_temp)?;
    rename(&link_temp, link_path)?;
    Ok(())
}

pub fn reflink_or_copy_with_warning(
    original: impl AsRef<Path>,
    new: impl AsRef<Path>,
) -> eyre::Result<()> {
    let original = original.as_ref();
    let new = new.as_ref();
    if let Some(written) = reflink_or_copy(original, new)? {
        warn!(
            ?original,
            ?new,
            "Had to copy {written} bytes manually because reflink copy failed"
        );
    }

    Ok(())
}
