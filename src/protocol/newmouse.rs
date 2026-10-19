//! MCHOSE new-mouse SDK protocol (`0x4D` / `getBattery` command `0x0900`).
//!
//! This is the protocol used by the A5 V3 Ultra+ (`0x3837:0x1018`) and other
//! current mice. It was extracted from the official web HUB SDK:
//!
//! ```text
//! output report 0x4D, 64 bytes:
//!   4D 01 01 <len> <cmd_lo> <cmd_hi> <biz> <seq> <data...> <xor>
//! 0x0900 = readDeviceInfo / getBattery
//!
//! input report 0x4D, payload (without report ID):
//!   ... 00 09 ... n[0..]
//!   n[0..1]  mouse vid (le16)
//!   n[2..3]  mouse pid (le16)
//!   n[10]    connect status
//!   n[11]    charge status
//!   n[12]    battery level
//!   n[13]    game mode
//! ```

use anyhow::{bail, Result};
use hidapi::{DeviceInfo, HidDevice};

use super::{charge_from_status, debug_raw, guess_connection, make_reading, validate_percent};
use crate::types::Reading;

pub const REPORT_ID: u8 = 0x4d;
pub const CMD_READ_INFO: u16 = 0x0900;

pub fn read(device: &HidDevice, info: &DeviceInfo) -> Result<Reading> {
    let path = info.path().to_string_lossy();
    let layout = crate::hid_layout::read_layout(device, Some(&path));

    // Only use this protocol when the interface actually has report 0x4D.
    let Some(size) = layout
        .as_ref()
        .and_then(|layout| layout.output_size(REPORT_ID))
    else {
        bail!("HUB 0x0900: report 0x4D is not present on this interface");
    };

    let packet = build_read_info_packet(size);
    let mut last_error = String::new();

    // The official HUB retries while the battery reads as 0 (the mouse may be
    // waking up). Try the query a few times, waiting up to ~1.8 s each time.
    for attempt in 0..3 {
        let written = device.write(&packet)?;
        if written == 0 {
            bail!("HUB 0x0900: device accepted zero bytes");
        }

        for _ in 0..6 {
            let mut buffer = [0u8; 128];
            let len = device.read_timeout(&mut buffer, 300)?;
            if len == 0 {
                continue;
            }
            debug_raw("NewMouse 0x4D input", &buffer[..len]);

            let parsed = parse_read_info(&buffer[..len]).or_else(|_| {
                parse_notification(&buffer[..len]).map(|(percent, charge)| NewMouseReading {
                    percent,
                    charge,
                    mouse_vid: info.vendor_id(),
                    mouse_pid: info.product_id(),
                    connect_status: 0,
                })
            });

            match parsed {
                Ok(parsed) => {
                    if parsed.percent > 0 || attempt == 2 {
                        return Ok(to_reading(parsed, info));
                    }
                    // 0% often means the mouse is still waking up; retry.
                    last_error = "battery reported 0%".to_string();
                    break;
                }
                Err(err) => last_error = format!("{err:#}"),
            }
        }
    }

    bail!("HUB 0x0900: no readDeviceInfo response ({last_error})")
}

fn to_reading(parsed: NewMouseReading, info: &DeviceInfo) -> Reading {
    let connection = guess_connection(info);
    make_reading(
        parsed.percent,
        parsed.charge,
        connection,
        "HUB 0x0900",
        Some(parsed.mouse_vid),
        Some(parsed.mouse_pid),
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewMouseReading {
    pub percent: u8,
    pub charge: crate::types::ChargeState,
    pub mouse_vid: u16,
    pub mouse_pid: u16,
    pub connect_status: u8,
}

/// Build the 64-byte output report for `getBattery` / `readDeviceInfo`.
pub fn build_read_info_packet(size: usize) -> Vec<u8> {
    let size = size.max(9);
    let mut packet = vec![0u8; size];
    packet[0] = REPORT_ID;
    packet[1] = 0x01;
    packet[2] = 0x01; // flags: checksum enabled
    packet[3] = 0x00; // data length
    packet[4] = (CMD_READ_INFO & 0xff) as u8;
    packet[5] = (CMD_READ_INFO >> 8) as u8;
    packet[6] = 0x00; // biz code
    packet[7] = 0x00; // sequence

    let mut checksum = 0u8;
    for byte in &packet[2..=7] {
        checksum ^= *byte;
    }
    packet[8] = checksum;
    packet
}

/// Parse an input report including the report ID (hidapi convention).
pub fn parse_read_info(buffer: &[u8]) -> Result<NewMouseReading> {
    if buffer.len() < 22 {
        bail!("HUB 0x0900 response too short: {}", buffer.len());
    }
    if buffer[0] != REPORT_ID {
        bail!("HUB 0x0900: unexpected report ID 0x{:02x}", buffer[0]);
    }

    // WebHID's event.data excludes the report ID; h mirrors that view.
    let h = &buffer[1..];
    if h[3] != 0x00 || h[4] != 0x09 {
        bail!("HUB 0x0900: unexpected command 0x{:02x}{:02x}", h[4], h[3]);
    }

    let n = &h[7..];
    if n.len() < 14 {
        bail!("HUB 0x0900 payload too short: {}", n.len());
    }

    let mouse_vid = u16::from_le_bytes([n[0], n[1]]);
    let mouse_pid = u16::from_le_bytes([n[2], n[3]]);
    let connect_status = n[10];
    let charge_raw = n[11];
    let percent = n[12];
    let _game_mode = n[13];

    // Error/empty responses (dock idle, mouse powered off) carry 0xff/0 in the
    // ID fields. Do not surface those as a 0% battery.
    if mouse_vid == 0 || mouse_vid == 0xffff || mouse_pid == 0 || mouse_pid == 0xffff {
        bail!("HUB 0x0900: invalid mouse id {mouse_vid:04x}:{mouse_pid:04x}");
    }

    if percent > 100 {
        bail!("HUB 0x0900: invalid battery percentage {percent}");
    }
    validate_percent(percent)?;

    Ok(NewMouseReading {
        percent,
        charge: charge_from_status(charge_raw),
        mouse_vid,
        mouse_pid,
        connect_status,
    })
}

/// Parse the unsolicited `0x0600` new-mouse notification. The official SDK
/// uses flags in `s[0]`/`s[1]`; battery is `s[3]`, charge status is `s[12]`.
pub fn parse_notification(buffer: &[u8]) -> Result<(u8, crate::types::ChargeState)> {
    if buffer.len() < 22 {
        bail!("HUB 0x0600 notification too short: {}", buffer.len());
    }
    if buffer[0] != REPORT_ID {
        bail!("HUB 0x0600: unexpected report ID 0x{:02x}", buffer[0]);
    }

    let h = &buffer[1..];
    if h[3] != 0x00 || h[4] != 0x06 {
        bail!("HUB 0x0600: unexpected command 0x{:02x}{:02x}", h[4], h[3]);
    }

    let s = &h[7..];
    if s.len() < 13 {
        bail!("HUB 0x0600 payload too short: {}", s.len());
    }

    let flags = s[0];
    let flags2 = s[1];
    if flags & 0x02 == 0 {
        bail!("HUB 0x0600: no battery flag");
    }

    let percent = s[3];
    if percent == 0 {
        bail!("HUB 0x0600: battery unknown (0)");
    }
    validate_percent(percent)?;

    let charge = if flags2 & 0x02 != 0 {
        charge_from_status(s[12])
    } else {
        crate::types::ChargeState::Unknown
    };

    Ok((percent, charge))
}
