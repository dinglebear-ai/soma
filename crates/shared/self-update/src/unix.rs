use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

/// Builds a restart command without consulting global process arguments.
pub fn restart_command(executable: &Path, args: impl IntoIterator<Item = OsString>) -> Command {
    let mut command = Command::new(executable);
    command.args(args);
    command
}

/// Replaces the current Unix process with the installed executable.
///
/// Adopters supervised by systemd or another process manager may instead exit
/// with an agreed restart status after receiving either `InstallOutcome` variant.
#[cfg(unix)]
pub fn reexec(
    executable: &Path,
    args: impl IntoIterator<Item = OsString>,
) -> crate::Result<std::convert::Infallible> {
    use std::os::unix::process::CommandExt;

    let error = restart_command(executable, args).exec();
    Err(crate::UpdateError::io(executable, error))
}
