//! fx-setup (task T4.3): fx's own installer. The interface is GPUI with
//! fx's design tokens (`main.rs`); this library holds what is tested on its
//! own: building and reading the package, and the install / uninstall
//! transaction. Windows calls go through fx-shell (no `unsafe` here).
#![forbid(unsafe_code)]

pub mod pack;
pub mod transaction;
