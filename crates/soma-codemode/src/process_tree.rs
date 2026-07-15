#[cfg(not(any(unix, windows)))]
pub mod noop;
#[cfg(unix)]
pub mod unix;
#[cfg(windows)]
pub mod windows;

pub fn terminate_process_tree(pid: u32) {
    #[cfg(unix)]
    unix::terminate_process_tree(pid);
    #[cfg(windows)]
    windows::terminate_process_tree(pid);
    #[cfg(not(any(unix, windows)))]
    noop::terminate_process_tree(pid);
}
