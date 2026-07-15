pub fn terminate_process_tree(pid: u32) {
    use nix::sys::signal::Signal;
    use nix::unistd::Pid;
    let _ = nix::sys::signal::killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
}
