use std::{
    ffi::OsStr,
    fs::File,
    io::Write,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    os::unix::fs::PermissionsExt,
    process::Command,
    sync::{LazyLock, Mutex},
};

use jane_eyre::eyre::{self, Context};
use mktemp::Temp;
use tracing::debug;

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
