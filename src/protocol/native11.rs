//! Native MCHOSE report `0x11` protocol.
//!
//! Used by A7 V2 Ultra, L7 Ultra and several newer 0x3837 devices. The payload
//! is XOR-obfuscated with `0xff`. The protocol is documented by the public
//! `bdfd9/mchose-battery` Linux HID driver:
//!
//! ```text
//! SET_FEATURE id=0x11, cmd=0x06, then XOR bytes 1.. with 0xff
//! wait ~2 s, GET_FEATURE id=0x11, XOR again
//! payload: command, vid(le16), pid(le16), fw(le32), flags, battery, charge
//! ```

use std::time::Duration;

use anyhow::{bail, Result};
use hidapi::{DeviceInfo, HidDevice};

use super::{
    charge_from_status, connection_from_flags, debug_raw, guess_connection, make_reading,
    validate_percent,
};
use crate::types::{ChargeState, Connection, Reading};

pub const REPORT_ID: u8 = 0x11;

#[derive(Debug, Clone)]
pub struct NativeReading {
    pub percent: u8,
    pub charge: ChargeState,
    pub connection: Connection,
    pub mouse_vid: Option<u16>,
    pub mouse_pid: Option<u16>,
    /// True when the mouse answered "sleeping" or "powered off".
    pub asleep: bool,
}

pub fn read(device: &HidDevice, info: &DeviceInfo) -> Result<Reading> {
    let mut errors = Vec::new();

    // Only use report lengths that actually exist in this interface's HID
    // descriptor. Requesting a wrong feature length can block hidraw forever.
    let path = info.path().to_string_lossy();
    let layout = crate::hid_layout::read_layout(device, Some(&path));
    let mut attempts: Vec<(usize, bool)> = Vec::new();
    if let Some(layout) = &layout {
        if let Some(size) = layout.feature_size(REPORT_ID) {
            attempts.push((size, true));
        }
        if let Some(size) = layout.output_size(REPORT_ID) {
            attempts.push((size, false));
        }
    }
    if attempts.is_empty() {
        // Descriptor unavailable (some Windows setups): use the known size.
        attempts.push((21, true));
    }

    for (size, feature) in attempts {
        if feature {
            if let Err(err) = send_feature(device, size) {
                errors.push(format!("feature {size}B: {err:#}"));
                continue;
            }

            // The official driver waits two seconds. Try a shorter wait first
            // so the tray starts quickly, then give the device the full time.
            std::thread::sleep(Duration::from_millis(250));
            match get_and_parse(device, info, size) {
                Ok(reading) => return Ok(reading),
                Err(err) => errors.push(format!("feature {size}B first read: {err:#}")),
            }

            std::thread::sleep(Duration::from_millis(1_650));
            match get_and_parse(device, info, size) {
                Ok(reading) => return Ok(reading),
                Err(err) => errors.push(format!("feature {size}B retry: {err:#}")),
            }
        } else {
            if let Err(err) = send_output(device, size) {
                errors.push(format!("output {size}B: {err:#}"));
                continue;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        if let Ok(reading) = read_input(device, info, size) {
            return Ok(reading);
        }
    }

    bail!("Native 0x11: {}", errors.join("; "))
}

fn build_report(size: usize) -> Vec<u8> {
    let mut report = vec![0xff; size.max(2)];
    report[0] = REPORT_ID;
    // 0x06 ^ 0xff, remaining payload bytes are 0x00 ^ 0xff.
    report[1] = 0x06 ^ 0xff;
    report
}

fn send_feature(device: &HidDevice, size: usize) -> Result<()> {
    let report = build_report(size);
    device.send_feature_report(&report)?;
    Ok(())
}

fn send_output(device: &HidDevice, size: usize) -> Result<()> {
    let report = build_report(size);
    let written = device.write(&report)?;
    if written == 0 {
        bail!("device accepted zero bytes");
    }
    Ok(())
}

fn get_and_parse(device: &HidDevice, info: &DeviceInfo, size: usize) -> Result<Reading> {
    let mut response = vec![0u8; size];
    response[0] = REPORT_ID;
    let len = device.get_feature_report(&mut response)?;
    if len == 0 {
        bail!("empty feature response");
    }
    debug_raw("native11 feature", &response[..len]);
    parse_native11(&response[..len]).map(|parsed| to_reading(parsed, info))
}

fn read_input(device: &HidDevice, info: &DeviceInfo, _size: usize) -> Result<Reading> {
    for _ in 0..5 {
        let mut buffer = [0u8; 65];
        let len = device.read_timeout(&mut buffer, 400)?;
        if len == 0 {
            continue;
        }
        debug_raw("native11 input", &buffer[..len]);
        if let Ok(parsed) = parse_native11(&buffer[..len]) {
            return Ok(to_reading(parsed, info));
        }
    }
    bail!("no valid 0x11 input report")
}

/// Parse a response including the report ID (or a bare 20-byte payload).
pub fn parse_native11(buffer: &[u8]) -> Result<NativeReading> {
    if buffer.len() < 12 {
        bail!("response too short: {}", buffer.len());
    }

    let data = if buffer[0] == REPORT_ID {
        &buffer[1..]
    } else {
        buffer
    };
    if data.len() < 12 {
        bail!("payload too short: {}", data.len());
    }

    // Undo the vendor's XOR obfuscation.
    let payload: Vec<u8> = data.iter().map(|byte| byte ^ 0xff).collect();
    if payload.len() < 12 {
        bail!("decoded payload too short: {}", payload.len());
    }

    let command = payload[0];
    if command != 0x06 {
        bail!("unexpected command byte 0x{command:02x}");
    }

    let vid = u16::from_le_bytes([payload[1], payload[2]]);
    let pid = u16::from_le_bytes([payload[3], payload[4]]);
    let flags = payload[9];
    let percent = payload[10];
    let charge_raw = payload[11];

    let mode = flags & 0x07;
    if mode == 6 || mode == 7 {
        bail!("mouse is sleeping or powered off");
    }
    if vid == 0 || vid == 0xffff {
        bail!("mouse is powered off (invalid vid)");
    }
    if charge_raw == 0xff {
        bail!("invalid charge status 0xff");
    }

    validate_percent(percent)?;

    Ok(NativeReading {
        percent,
        charge: charge_from_status(charge_raw),
        connection: connection_from_flags(flags),
        mouse_vid: Some(vid),
        mouse_pid: Some(pid),
        asleep: false,
    })
}

fn to_reading(parsed: NativeReading, info: &DeviceInfo) -> Reading {
    let connection = if parsed.connection == Connection::Unknown {
        guess_connection(info)
    } else {
        parsed.connection
    };
    make_reading(
        parsed.percent,
        parsed.charge,
        connection,
        "Native 0x11",
        parsed.mouse_vid,
        parsed.mouse_pid,
    )
}
