use std::{
    ffi::OsStr,
    fs::File,
    io::{BufRead, BufReader, Read, Write},
    marker::PhantomData,
    ops::{Deref, DerefMut},
    os::unix::fs::PermissionsExt,
    process::Command,
    str,
    sync::{LazyLock, Mutex},
};

use jane_eyre::eyre::{self, Context};
use mktemp::Temp;
use tracing::{debug, info, trace, warn};

/// Global instance of [Shell] for single-threaded situations.
pub static SHELL: LazyLock<Mutex<Shell>> =
    LazyLock::new(|| Mutex::new(Shell::new().expect("Failed to create Shell")));

/// Runs shell scripts by writing their contents to a temporary file.
///
/// This lets us compile shell scripts into the program binary, which is useful for two reasons:
/// - The program can be run from any working directory without breaking the shell scripts
/// - You can edit shell scripts while they are running without interfering with their execution
///   (usually the shell will read the next command from the same offset in the new file)
#[derive(Debug)]
pub struct Shell(Temp);
impl Shell {
    pub fn new() -> eyre::Result<Self> {
        let result = Temp::new_file().wrap_err("Failed to create temporary file")?;
        let mut permissions = std::fs::metadata(&result)
            .wrap_err("Failed to get metadata")?
            .permissions();
        permissions.set_mode(permissions.mode() | 0b001001001);
        std::fs::set_permissions(&result, permissions).wrap_err("Failed to set permissions")?;

        Ok(Self(result))
    }

    /// Get a handle that wraps a [Command] that can run the given code.
    ///
    /// Each instance can only run one script at a time, hence the `&mut self`.
    #[tracing::instrument(level = "error", skip_all)]
    pub fn run<S: AsRef<OsStr>>(
        &mut self,
        code: &str,
        args: impl IntoIterator<Item = S>,
    ) -> eyre::Result<ShellHandle> {
        let path = self.0.as_path();
        let args = args
            .into_iter()
            .map(|arg| arg.as_ref().to_owned())
            .collect::<Vec<_>>();
        debug!(?path, ?args, "Running script");
        let mut file = File::create(&self.0).wrap_err("Failed to create shell script")?;
        file.write_all(code.as_bytes())
            .wrap_err("Failed to write shell script")?;

        let mut result = Command::new(&*self.0);
        result.args(args);

        Ok(ShellHandle(result, PhantomData))
    }
}

#[derive(Debug)]
pub struct ShellHandle<'shell>(Command, PhantomData<&'shell mut Shell>);

impl Deref for ShellHandle<'_> {
    type Target = Command;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for ShellHandle<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

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
