use std::{ffi::OsString, fs};

use super::{resolve_sidecar_command_with_env, sidecar_base_env};

#[test]
fn resolves_bare_command_from_parent_path_for_env_cleared_spawn() {
    let dir = std::env::temp_dir().join(format!(
        "soma-sidecar-command-resolution-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("test command dir");

    let command_path = if cfg!(windows) {
        dir.join("fake-python.EXE")
    } else {
        dir.join("fake-python")
    };

    fs::write(&command_path, b"").expect("fake command");
    let path_env = std::env::join_paths([&dir]).expect("PATH value");
    let pathext_env = Some(OsString::from(".EXE;.CMD"));

    let resolved = resolve_sidecar_command_with_env("fake-python", Some(path_env), pathext_env);

    assert_eq!(resolved, command_path);
    let _ = fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn resolves_mise_shims_to_real_runtime() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let dir = std::env::temp_dir().join(format!(
        "soma-sidecar-mise-resolution-{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("test command dir");

    let real_command = dir.join("real-node");
    fs::write(&real_command, b"").expect("real command");

    let mise_path = dir.join("mise");
    fs::write(
        &mise_path,
        format!(
            "#!/bin/sh\nif [ \"$1\" = \"which\" ] && [ \"$2\" = \"fake-node\" ]; then printf '%s\\n' {:?}; exit 0; fi\nexit 1\n",
            real_command.display().to_string()
        ),
    )
    .expect("fake mise");
    let mut permissions = fs::metadata(&mise_path)
        .expect("fake mise metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&mise_path, permissions).expect("fake mise executable");

    let shim_path = dir.join("fake-node");
    symlink(&mise_path, &shim_path).expect("fake shim");

    let path_env = std::env::join_paths([&dir]).expect("PATH value");
    let resolved = resolve_sidecar_command_with_env("fake-node", Some(path_env), None);

    assert_eq!(resolved, real_command);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sidecar_base_env_does_not_leak_parent_path() {
    let keys: Vec<_> = sidecar_base_env()
        .into_iter()
        .map(|(key, _)| key.to_string_lossy().to_ascii_uppercase())
        .collect();

    assert!(!keys.iter().any(|key| key == "PATH"));
}

#[test]
fn leaves_explicit_command_paths_unresolved() {
    let resolved = resolve_sidecar_command_with_env(
        "./fake-python",
        Some(OsString::from("/tmp")),
        Some(OsString::from(".EXE")),
    );

    assert_eq!(resolved, std::path::PathBuf::from("./fake-python"));
}
