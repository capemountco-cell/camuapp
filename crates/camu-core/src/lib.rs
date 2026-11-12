//! Camu's logic: finding installed apps and the newest version of each.
//!
//! Everything that makes a decision lives here, not in the Tauri crate and not
//! in React. This crate does not depend on Tauri, so it builds and its tests
//! run on Linux in CI, without a window, a webview or a Mac.

pub mod app;
pub mod check;
pub mod fetch;
pub mod icon;
pub mod launch;
pub mod macos;
pub mod notes;
pub mod platform;
pub mod scan;
pub mod sources;
pub mod update;
pub mod version;

pub use app::InstalledApp;
pub use check::{Checker, Environment, Install, Report, Status};
