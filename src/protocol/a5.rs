//! Legacy A5 / AX5 protocol (VID 0x2023, vendor interface 2).
//!
//! The old DriverCore uses 65-byte feature reports. A battery query is
//! `report[3]=2, report[4]=2, report[6]=0x83`; the answer carries the charge
//! state in byte 7 and the percentage in byte 8.

use std::time::Duration;

use anyhow::{bail, Result};
use hidapi::{DeviceInfo, HidDevice};

use super::{charge_from_status, debug_raw, guess_connection, make_reading, validate_percent};
use crate::types::Reading;

const REPORT_LEN: usize = 65;
const CMD_BATTERY: u8 = 0x83;

pub fn read(device: &HidDevice, info: &DeviceInfo) -> Result<Reading> {
    let mut report = vec![0u8; REPORT_LEN];
    report[3] = 0x02;
    report[4] = 0x02;
    report[6] = CMD_BATTERY;

    device.send_feature_report(&report)?;
    std::thread::sleep(Duration::from_millis(60));

    let mut response = vec![0u8; REPORT_LEN];
    response[3] = 0x02;
    let len = device.get_feature_report(&mut response)?;
    if len == 0 {
        bail!("empty A5 feature response");
    }
    debug_raw("A5/AX5", &response[..len]);

    let (percent, charge_raw) = parse_a5(&response[..len])?;
    Ok(make_reading(
        percent,
        charge_from_status(charge_raw),
        guess_connection(info),
        "A5/AX5",
        None,
        None,
    ))
}

/// Returns `(percentage, charge_status)`.
pub fn parse_a5(buffer: &[u8]) -> Result<(u8, u8)> {
    if buffer.len() < 9 {
        bail!("A5 response too short: {}", buffer.len());
    }
    let charge = buffer[7];
    let percent = buffer[8];
    if percent == 0xff {
        bail!("invalid A5 percentage 0xff");
    }
    validate_percent(percent)?;
    if charge == 0xff {
        bail!("invalid A5 charge status 0xff");
    }
    Ok((percent, charge))
}
