//! Older A7 protocol (`0x5253:0x1021`).
//!
//! It is very close to [`super::native11`], but the response parser is more
//! lenient: some A7 firmwares leave the VID/PID fields empty, so we only look
//! at the battery and charge bytes. Native 0x11 is attempted first; this module
//! is the fallback for devices that reject the VID check.

use std::time::Duration;

use anyhow::{bail, Result};
use hidapi::{DeviceInfo, HidDevice};

use super::{charge_from_status, debug_raw, guess_connection, make_reading, validate_percent};
use crate::types::Reading;

const REPORT_ID: u8 = 0x11;
const QUERY_LEN: usize = 21;

pub fn read(device: &HidDevice, info: &DeviceInfo) -> Result<Reading> {
    let mut report = vec![0xffu8; QUERY_LEN];
    report[0] = REPORT_ID;
    report[1] = 0x06 ^ 0xff;

    device.send_feature_report(&report)?;
    std::thread::sleep(Duration::from_millis(200));

    let mut response = vec![0u8; 64];
    response[0] = REPORT_ID;
    let len = device.get_feature_report(&mut response)?;
    if len == 0 {
        bail!("empty A7 feature response");
    }
    debug_raw("A7 legacy", &response[..len]);

    let (percent, charge_raw) = parse_a7(&response[..len])?;
    Ok(make_reading(
        percent,
        charge_from_status(charge_raw),
        guess_connection(info),
        "A7 legacy",
        None,
        None,
    ))
}

/// Returns `(percentage, charge_status)`.
pub fn parse_a7(buffer: &[u8]) -> Result<(u8, u8)> {
    if buffer.len() < 13 {
        bail!("A7 response too short: {}", buffer.len());
    }

    let raw: Vec<u8> = buffer.iter().map(|byte| byte ^ 0xff).collect();

    // With a report ID the payload starts at byte 2, otherwise at byte 1.
    let starts: [usize; 2] = if buffer[0] == REPORT_ID {
        [2, 1]
    } else {
        [1, 2]
    };
    for start in starts {
        if raw.len() < start + 11 {
            continue;
        }
        let payload = &raw[start..];
        let percent = payload[9];
        let charge = payload[10];
        if percent <= 100 && charge != 0xff {
            return Ok((percent, charge));
        }
    }

    validate_percent(raw.get(11).copied().unwrap_or(0xff))?;
    bail!("A7 response did not contain a valid battery value")
}
