#![forbid(unsafe_code)]

use std::time::Duration;

#[cfg(unix)]
pub(crate) fn wait_fd_readable(fd: std::os::unix::io::BorrowedFd<'_>, timeout: Duration) -> bool {
    use nix::poll::{PollFd, PollFlags, poll};

    let timeout_ms: u16 = timeout.as_millis().min(u16::MAX as u128) as u16;
    let mut fds = [PollFd::new(fd, PollFlags::POLLIN)];
    match poll(&mut fds, timeout_ms) {
        Ok(0) => false,
        Ok(_) => true,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
pub(crate) fn wait_fd_readable(_fd: i32, _timeout: Duration) -> bool {
    true
}
