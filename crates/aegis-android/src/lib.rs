//! Android backend. The app never builds a tunnel itself: a small Kotlin
//! plugin (`android/`) asks Android's `VpnService` for one and runs the
//! official WireGuard tunnel library on it. This crate is the Rust side:
//! it stores profiles in the app's private folder and calls the plugin.
//!
//! Android's kill switch belongs to the system ("Block connections without
//! VPN"), so this backend reports it as unsupported rather than pretending.

#[cfg(target_os = "android")]
mod imp;
#[cfg(target_os = "android")]
pub use imp::{AndroidBackend, init};
