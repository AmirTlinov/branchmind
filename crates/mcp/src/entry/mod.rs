#![forbid(unsafe_code)]

pub(crate) mod framing;
mod stdio;

#[cfg(unix)]
mod shared;
#[cfg(unix)]
mod socket;

#[cfg(unix)]
pub(crate) use shared::{SharedProxyConfig, run_shared_proxy};
#[cfg(unix)]
pub(crate) use socket::{DaemonConfig, run_socket_daemon};
pub(crate) use stdio::run_stdio;
