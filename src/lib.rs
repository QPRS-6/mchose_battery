//! Cross-platform MCHOSE / 迈从 mouse battery reader.
//!
//! The library part is UI-independent so the HID protocols can be unit tested
//! with synthetic reports. The binary adds a Windows tray icon or a KDE
//! StatusNotifierItem.

pub mod autostart;
pub mod cli;
pub mod hid_layout;
pub mod icon;
pub mod protocol;
pub mod scan;
pub mod service;
pub mod types;

#[cfg(any(target_os = "linux", windows))]
pub mod platform;
#[cfg(any(target_os = "linux", windows))]
pub mod ui;
