//! MCHOSE `gmouse` web protocol family (G3 / G3 V2 / A7e / G7 …).
//!
//! The host writes `55 30 a5 0b 2e 01 01 01` and waits for an input report
//! beginning with `aa 30`. The battery is at offset +8 and the charge flag at
//! offset +9.

use anyhow::{bail, Result};
use hidapi::{DeviceInfo, HidDevice};

use super::{charge_from_status, debug_raw, guess_connection, make_reading, validate_percent};
use crate::types::Reading;

const REPORT_LEN: usize = 65;

pub fn read(device: &HidDevice, info: &DeviceInfo) -> Result<Reading> {
    let mut report = [0u8; REPORT_LEN];
    report[1] = 0x55;
    report[2] = 0x30;
    report[3] = 0xa5;
    report[4] = 0x0b;
    report[5] = 0x2e;
    report[6] = 0x01;
    report[7] = 0x01;
    report[8] = 0x01;

    let written = device.write(&report)?;
    if written == 0 {
        bail!("gmouse: device accepted zero bytes");
    }

    for _ in 0..8 {
        let mut buffer = [0u8; REPORT_LEN];
        let len = device.read_timeout(&mut buffer, 300)?;
        if len == 0 {
            continue;
        }
        debug_raw("Gmouse", &buffer[..len]);
        if let Ok((percent, charge_raw)) = parse_gmouse(&buffer[..len]) {
            return Ok(make_reading(
                percent,
                charge_from_status(charge_raw),
                guess_connection(info),
                "Gmouse",
                None,
                None,
            ));
        }
    }

    bail!("gmouse: no aa 30 response")
}

/// Returns `(percentage, charge_status)`.
pub fn parse_gmouse(buffer: &[u8]) -> Result<(u8, u8)> {
    let offset = if buffer.len() >= 2 && buffer[0] == 0xaa && buffer[1] == 0x30 {
        0
    } else if buffer.len() >= 3 && buffer[1] == 0xaa && buffer[2] == 0x30 {
        1
    } else {
        bail!("gmouse: response does not start with aa 30");
    };

    if buffer.len() < offset + 10 {
        bail!("gmouse response too short: {}", buffer.len());
    }

    let percent = buffer[offset + 8];
    let charge = buffer[offset + 9];
    if percent == 0xff {
        bail!("gmouse: invalid percentage 0xff");
    }
    validate_percent(percent)?;

    Ok((percent, charge))
}
