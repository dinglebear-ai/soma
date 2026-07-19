#![cfg(unix)]

use std::ffi::OsString;
use std::path::Path;

use soma_self_update::restart_command;

#[test]
fn restart_command_targets_installed_binary_and_preserves_arguments() {
    let args = [OsString::from("serve"), OsString::from("--port=3100")];
    let command = restart_command(Path::new("/opt/example/bin/example"), args.clone());
    assert_eq!(command.get_program(), "/opt/example/bin/example");
    assert_eq!(
        command.get_args().collect::<Vec<_>>(),
        args.iter().collect::<Vec<_>>()
    );
}
