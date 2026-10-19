//! HID battery protocols used by MCHOSE / 迈从 mice.
//!
//! The official M HUB is not required. We speak the vendor HID feature-report
//! protocols directly. Different generations use different report IDs, so the
//! scanner probes the protocols that are plausible for an interface and keeps
//! the first valid answer.

use std::sync::OnceLock;

use anyhow::{bail, Result};
use hidapi::DeviceInfo;

use crate::types::{product_looks_wireless, ChargeState, Connection, Reading};

pub mod a5;
pub mod a7;
pub mod gmouse;
pub mod hub;
pub mod native11;
pub mod newmouse;

pub use a5::parse_a5;
pub use a7::parse_a7;
pub use gmouse::parse_gmouse;
pub use hub::{parse_hub, ReportKind};
pub use native11::{parse_native11, NativeReading};

/// Reject nonsense percentages before they reach the tray.
pub fn validate_percent(percent: u8) -> Result<u8> {
    if percent > 100 {
        bail!("invalid battery percentage: {percent}");
    }
    Ok(percent)
}

pub fn charge_from_status(raw: u8) -> ChargeState {
    match raw {
        0 => ChargeState::NotCharging,
        1 => ChargeState::Charging,
        2 => ChargeState::Full,
        _ => ChargeState::Unknown,
    }
}

pub fn connection_from_flags(flags: u8) -> Connection {
    match flags & 0x07 {
        0 => Connection::Wired,
        1 => Connection::Wireless24G,
        2 => Connection::Bluetooth,
        _ => Connection::Unknown,
    }
}

/// Heuristic used when the protocol itself does not carry a link type.
pub fn guess_connection(info: &DeviceInfo) -> Connection {
    let product = info.product_string().unwrap_or("");
    if product_looks_wireless(product) {
        return Connection::Wireless24G;
    }
    if crate::types::is_dongle_pid(info.vendor_id(), info.product_id()) {
        return Connection::Wireless24G;
    }
    Connection::Wired
}

pub fn is_vendor_usage_page(page: u16) -> bool {
    page >= 0xff00
}

/// Print raw HID responses when `MCHOSE_DEBUG_RAW=1` is set.
pub fn debug_raw(label: &str, bytes: &[u8]) {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    if !*ENABLED.get_or_init(|| std::env::var_os("MCHOSE_DEBUG_RAW").is_some()) {
        return;
    }
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    eprintln!("[raw {label}] {hex}");
}

/// Build a `Reading` with the common fields filled in.
pub fn make_reading(
    percent: u8,
    charge: ChargeState,
    connection: Connection,
    protocol: &'static str,
    mouse_vid: Option<u16>,
    mouse_pid: Option<u16>,
) -> Reading {
    let mut reading = Reading::new(percent, charge, connection, protocol);
    reading.mouse_vid = mouse_vid;
    reading.mouse_pid = mouse_pid;
    reading
}

/// Connection of a response that has no explicit link field.
pub fn connection_for(info: &DeviceInfo, wireless_hint: bool) -> Connection {
    if wireless_hint {
        Connection::Wireless24G
    } else {
        guess_connection(info)
    }
}
