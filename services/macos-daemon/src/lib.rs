//! VPNFlow root daemon for macOS (launchd). See crates/aegis-macos for the
//! client and docs/development/MACOS.md for the install and test plan.

#[cfg(unix)]
pub mod handler;
pub mod netcfg;
#[cfg(unix)]
pub mod server;
#[cfg(unix)]
pub mod store;
#[cfg(unix)]
pub mod tunnel;
pub mod uapi;
