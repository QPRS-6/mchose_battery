//! MCHOSE web-HUB protocol family.
//!
//! Newer mice (A5 Ultra/V2/V3, A7 V2+, AX5 V2, G3 Ultra, K7/M7/L7 …) answer
//! feature reports `0x06` (wired) or `0x13` (2.4G). The battery packet contains
//! `command=10, type=5` followed by a percentage and a status nibble.
//!
//! This protocol was ported from the public `aliezzzz/mchose-battery` project
//! and the official web HUB.

use std::time::Duration;

use anyhow::{bail, Result};
use hidapi::{DeviceInfo, HidDevice};

use super::{debug_raw, make_reading, validate_percent};
use crate::types::{is_dongle_pid, product_looks_wireless, ChargeState, Connection, Reading};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportKind {
    Wireless13,
    Wired06,
}

impl ReportKind {
    fn report_id(self) -> u8 {
        match self {
            ReportKind::Wireless13 => 0x13,
            ReportKind::Wired06 => 0x06,
        }
    }

    fn connection(self) -> Connection {
        match self {
            ReportKind::Wireless13 => Connection::Wireless24G,
            ReportKind::Wired06 => Connection::Wired,
        }
    }

    fn protocol(self) -> &'static str {
        match self {
            ReportKind::Wireless13 => "Hub 0x13",
            ReportKind::Wired06 => "Hub 0x06",
        }
    }

    fn sizes(self) -> &'static [usize] {
        match self {
            // 20-byte payload on A5 V3, 19-byte payload on 41e4 receivers,
            // and 64-byte payload on older HUB devices.
            ReportKind::Wireless13 => &[21, 20, 65, 64],
            ReportKind::Wired06 => &[65, 21, 64, 20],
        }
    }
}

#[derive(Debug, Clone)]
pub struct HubReading {
    pub percent: u8,
    pub charge: ChargeState,
    pub connection: Connection,
    pub protocol: &'static str,
}

pub fn read(device: &HidDevice, info: &DeviceInfo) -> Result<Reading> {
    let mut errors = Vec::new();
    for kind in [ReportKind::Wireless13, ReportKind::Wired06] {
        match read_kind(device, info, kind) {
            Ok(reading) => return Ok(reading),
            Err(err) => errors.push(format!("{err:#}")),
        }
    }
    bail!("{}", errors.join("; "))
}

pub fn read_kind(device: &HidDevice, info: &DeviceInfo, kind: ReportKind) -> Result<Reading> {
    let mut errors = Vec::new();
    let mut attempts: Vec<(usize, bool)> = Vec::new();

    // Use the real report sizes from the HID descriptor. This is important on
    // Linux: a GET_FEATURE with a length the device does not implement can
    // block in the kernel indefinitely.
    let path = info.path().to_string_lossy();
    let layout = crate::hid_layout::read_layout(device, Some(&path));
    if let Some(layout) = &layout {
        let report_id = kind.report_id();
        let feature = layout.feature_size(report_id);
        let output = layout.output_size(report_id);

        if kind == ReportKind::Wireless13 {
            // 0x13 is input/output on many receivers and feature-capable on
            // wired A5 V3. The output+input path is always safe and is what
            // the receiver uses, so try it first.
            if let Some(size) = output {
                attempts.push((size, false));
            }
            if let Some(size) = feature {
                attempts.push((size, true));
            }
        } else {
            if let Some(size) = feature {
                attempts.push((size, true));
            }
            if let Some(size) = output {
                attempts.push((size, false));
            }
        }
    } else {
        attempts.extend(kind.sizes().iter().map(|&size| (size, true)));
    }

    if attempts.is_empty() {
        bail!(
            "{}: report ID not present in HID descriptor",
            kind.protocol()
        );
    }

    for (size, feature) in attempts {
        if feature {
            match send_feature(device, kind, size) {
                Ok(()) => {
                    std::thread::sleep(Duration::from_millis(120));
                    if let Ok(reading) = get_and_parse(device, info, kind, size) {
                        return Ok(reading);
                    }
                    if let Ok(reading) = read_input(device, info, kind) {
                        return Ok(reading);
                    }
                    errors.push(format!(
                        "{} feature {size}B: no valid response",
                        kind.protocol()
                    ));
                }
                Err(err) => {
                    errors.push(format!("{} feature {size}B: {err:#}", kind.protocol()));
                    if let Ok(reading) = try_output(device, info, kind, size) {
                        return Ok(reading);
                    }
                }
            }
        } else if let Ok(reading) = try_output(device, info, kind, size) {
            return Ok(reading);
        } else {
            errors.push(format!(
                "{} output {size}B: no valid response",
                kind.protocol()
            ));
        }
    }

    if errors.is_empty() {
        bail!("{}: no battery response", kind.protocol());
    }
    bail!("{}", errors.join("; "))
}

fn build_report(kind: ReportKind, size: usize) -> Vec<u8> {
    let mut report = vec![0u8; size.max(8)];
    report[0] = kind.report_id();
    match kind {
        ReportKind::Wireless13 => {
            report[1] = 0x4a;
            report[2] = 0x01;
        }
        ReportKind::Wired06 => {
            report[1] = 0x87;
            report[4] = 0x01;
            report[6] = 0x02;
        }
    }
    report
}

fn send_feature(device: &HidDevice, kind: ReportKind, size: usize) -> Result<()> {
    device.send_feature_report(&build_report(kind, size))?;
    Ok(())
}

fn get_and_parse(
    device: &HidDevice,
    info: &DeviceInfo,
    kind: ReportKind,
    size: usize,
) -> Result<Reading> {
    let mut response = vec![0u8; size];
    response[0] = kind.report_id();
    let len = device.get_feature_report(&mut response)?;
    if len == 0 {
        bail!("empty feature response");
    }
    debug_raw(kind.protocol(), &response[..len]);
    parse_hub(&response[..len], kind).map(|parsed| to_reading(parsed, info))
}

fn try_output(
    device: &HidDevice,
    info: &DeviceInfo,
    kind: ReportKind,
    size: usize,
) -> Result<Reading> {
    let report = build_report(kind, size);
    let written = device.write(&report)?;
    if written == 0 {
        bail!("device accepted zero bytes");
    }
    std::thread::sleep(Duration::from_millis(100));
    read_input(device, info, kind)
}

fn read_input(device: &HidDevice, info: &DeviceInfo, kind: ReportKind) -> Result<Reading> {
    for _ in 0..6 {
        let mut buffer = [0u8; 65];
        let len = device.read_timeout(&mut buffer, 300)?;
        if len == 0 {
            continue;
        }
        debug_raw(&format!("{} input", kind.protocol()), &buffer[..len]);
        if let Ok(parsed) = parse_hub(&buffer[..len], kind) {
            return Ok(to_reading(parsed, info));
        }
    }
    bail!("no valid {} input report", kind.protocol())
}

fn to_reading(parsed: HubReading, info: &DeviceInfo) -> Reading {
    // Report 0x13 is the wireless channel on receivers, but the A5 V3 wired
    // interface also exposes 0x13 for the same HUB command. Prefer the actual
    // USB link type when the device is a wired 0x3837 mouse.
    let connection = if parsed.connection == Connection::Wireless24G
        && info.vendor_id() == 0x3837
        && !product_looks_wireless(info.product_string().unwrap_or(""))
        && !is_dongle_pid(info.vendor_id(), info.product_id())
    {
        Connection::Wired
    } else {
        parsed.connection
    };

    make_reading(
        parsed.percent,
        parsed.charge,
        connection,
        parsed.protocol,
        None,
        None,
    )
}

/// Parse a battery response. `kind` tells us whether the packet is expected to
/// contain the three-byte wireless padding after the command byte.
pub fn parse_hub(buffer: &[u8], kind: ReportKind) -> Result<HubReading> {
    if buffer.len() < 3 {
        bail!("response too short: {}", buffer.len());
    }

    let expected_id = kind.report_id();
    if buffer[0] != expected_id && !matches!(buffer[0], 0x0a | 0x4a) {
        bail!(
            "unexpected report ID 0x{:02x} for {}",
            buffer[0],
            kind.protocol()
        );
    }

    let wireless = kind == ReportKind::Wireless13;
    let spaces: [usize; 2] = if wireless { [3, 0] } else { [0, 3] };
    let data = if buffer[0] == expected_id {
        &buffer[1..]
    } else {
        buffer
    };

    // A5 V3 / newer receiver response:
    //   13 4a 01 00 02 64 01 00 ... <sum>
    // The last byte is the low byte of the sum of all preceding bytes.
    // 0x64 is the battery percentage, 0x01 is the charge status.
    if checksum_ok(buffer) {
        if let Some((percent, charge)) = parse_4a_battery(data) {
            return Ok(HubReading {
                percent,
                charge,
                connection: kind.connection(),
                protocol: kind.protocol(),
            });
        }
    }

    // The web HUB's `ilt` parser. If a command=10/type=5 packet is present but
    // its payload is invalid, do not reinterpret the bytes with the BBE parser.
    let mut saw_ilt = false;
    let mut ilt_error = None;
    for space in spaces {
        if let Some((percent, packed)) = find_ilt(buffer, space) {
            saw_ilt = true;
            match charge_from_packed(percent, packed) {
                Ok(charge) => {
                    return Ok(HubReading {
                        percent,
                        charge,
                        connection: kind.connection(),
                        protocol: kind.protocol(),
                    });
                }
                Err(err) => ilt_error = Some(err),
            }
        }
    }
    if saw_ilt {
        bail!(
            "invalid command=10 type=5 packet: {}",
            ilt_error
                .map(|err| format!("{err:#}"))
                .unwrap_or_else(|| "unknown".to_string())
        );
    }

    // Some HUB variants put the values at a fixed offset (the web code calls
    // this parser `Bbe`). Try the expected base first, then fallbacks.
    let bases: [usize; 4] = if wireless { [0, 1, 6, 7] } else { [6, 7, 0, 1] };
    for base in bases {
        if data.len() < base + 2 {
            continue;
        }
        let percent = data[base];
        let packed = data[base + 1];
        if let Ok(charge) = charge_from_packed(percent, packed) {
            return Ok(HubReading {
                percent,
                charge,
                connection: kind.connection(),
                protocol: kind.protocol(),
            });
        }
    }

    bail!(
        "no command=10 type=5 battery packet found ({} bytes)",
        buffer.len()
    )
}

/// The A5 V3 / newer receiver answers:
/// `13 4a 01 00 02 64 01 00 ... <sum>`.
/// The final byte is a simple 8-bit sum of all preceding bytes.
fn checksum_ok(packet: &[u8]) -> bool {
    if packet.len() < 3 {
        return false;
    }
    let sum: u32 = packet[..packet.len() - 1]
        .iter()
        .map(|&byte| byte as u32)
        .sum();
    (sum & 0xff) as u8 == packet[packet.len() - 1]
}

fn parse_4a_battery(data: &[u8]) -> Option<(u8, ChargeState)> {
    if data.len() < 6 {
        return None;
    }
    if data[0] != 0x4a || data[1] != 0x01 || data[3] != 0x02 {
        return None;
    }

    let percent = data[4];
    if percent > 100 {
        return None;
    }
    let charge = match data[5] {
        0 => ChargeState::NotCharging,
        1 => ChargeState::Charging,
        2 => ChargeState::Full,
        _ => ChargeState::Unknown,
    };
    Some((percent, charge))
}

fn find_ilt(buffer: &[u8], space: usize) -> Option<(u8, u8)> {
    for start in 0..buffer.len() {
        if buffer[start] != 10 {
            continue;
        }
        let type_idx = start + 1 + space;
        if buffer.get(type_idx).copied() != Some(5) {
            continue;
        }
        let battery_idx = type_idx + 1;
        let packed_idx = type_idx + 2;
        if packed_idx >= buffer.len() {
            continue;
        }
        return Some((buffer[battery_idx], buffer[packed_idx]));
    }
    None
}

fn charge_from_packed(percent: u8, packed: u8) -> Result<ChargeState> {
    validate_percent(percent)?;
    if packed == 0xff {
        bail!("invalid status byte 0xff");
    }

    let full_status = packed & 0x0f;
    let charge_status = packed >> 4;

    let charge = if percent >= 99 && full_status != 0 {
        ChargeState::Full
    } else {
        match charge_status {
            0 => match full_status {
                2 => ChargeState::Full,
                1 if percent >= 95 => ChargeState::Full,
                _ => ChargeState::NotCharging,
            },
            1 => ChargeState::Charging,
            2 => ChargeState::Full,
            0x0f => ChargeState::Unknown,
            _ => ChargeState::Unknown,
        }
    };

    Ok(charge)
}
