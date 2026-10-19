//! Minimal HID report-descriptor parser.
//!
//! MCHOSE devices use different report lengths for the same report ID depending
//! on the model (for example 0x11 is 20 bytes on A5 V3 but 64 bytes on some
//! older devices). Asking hidraw for a feature report with the wrong length can
//! block indefinitely, so we read the descriptor and only use real lengths.

use std::collections::BTreeMap;

use hidapi::HidDevice;

#[derive(Clone, Debug, Default)]
pub struct ReportLayout {
    pub feature: BTreeMap<u8, u16>,
    pub output: BTreeMap<u8, u16>,
    pub input: BTreeMap<u8, u16>,
}

impl ReportLayout {
    /// Length including the report ID byte.
    pub fn feature_size(&self, report_id: u8) -> Option<usize> {
        self.feature
            .get(&report_id)
            .map(|&bytes| bytes as usize + 1)
    }

    pub fn output_size(&self, report_id: u8) -> Option<usize> {
        self.output.get(&report_id).map(|&bytes| bytes as usize + 1)
    }

    #[allow(dead_code)]
    pub fn input_size(&self, report_id: u8) -> Option<usize> {
        self.input.get(&report_id).map(|&bytes| bytes as usize + 1)
    }
}

pub fn read_layout(device: &HidDevice, path: Option<&str>) -> Option<ReportLayout> {
    // On Linux, read the sysfs descriptor first. It is a plain file read and
    // cannot block in the hidraw driver (the HIDIOCGRDESC ioctl can).
    #[cfg(target_os = "linux")]
    if let Some(path) = path {
        use std::path::Path;
        if let Some(name) = Path::new(path).file_name() {
            let descriptor = Path::new("/sys/class/hidraw")
                .join(name)
                .join("device/report_descriptor");
            match std::fs::read(descriptor) {
                Ok(bytes) if !bytes.is_empty() => return Some(parse_descriptor(&bytes)),
                Ok(_) => {}
                Err(err) => {
                    if std::env::var_os("MCHOSE_DEBUG_RAW").is_some() {
                        eprintln!("[layout] sysfs descriptor failed: {err}");
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    let _ = path;

    let mut buffer = [0u8; 4096];
    match device.get_report_descriptor(&mut buffer) {
        Ok(length) if length > 0 => Some(parse_descriptor(&buffer[..length])),
        Ok(_) => None,
        Err(err) => {
            if std::env::var_os("MCHOSE_DEBUG_RAW").is_some() {
                eprintln!("[layout] get_report_descriptor failed: {err}");
            }
            None
        }
    }
}

/// Parse Global Report ID / Report Size / Report Count and Main Input /
/// Output / Feature items. Long items and local items are skipped.
pub fn parse_descriptor(bytes: &[u8]) -> ReportLayout {
    let mut layout = ReportLayout::default();
    let mut report_id = 0u8;
    let mut report_size = 0u32;
    let mut report_count = 0u32;
    let mut position = 0usize;

    while position < bytes.len() {
        let prefix = bytes[position];
        position += 1;

        if prefix == 0xfe {
            // Long item: data size, tag, data.
            if position + 2 > bytes.len() {
                break;
            }
            let data_size = bytes[position] as usize;
            position += 2 + data_size;
            continue;
        }

        let size = match prefix & 0x03 {
            0 => 0,
            1 => 1,
            2 => 2,
            _ => 4,
        };
        if position + size > bytes.len() {
            break;
        }

        let mut value = 0u32;
        for offset in 0..size {
            value |= (bytes[position + offset] as u32) << (8 * offset);
        }
        position += size;

        let tag = prefix >> 4;
        let item_type = (prefix >> 2) & 0x03;

        match (item_type, tag) {
            // Global items.
            (1, 0x8) => report_id = value as u8,
            (1, 0x7) => report_size = value,
            (1, 0x9) => report_count = value,
            // Main items.
            (0, 0x8) => add_field(&mut layout.input, report_id, report_size, report_count),
            (0, 0x9) => add_field(&mut layout.output, report_id, report_size, report_count),
            (0, 0xb) => add_field(&mut layout.feature, report_id, report_size, report_count),
            _ => {}
        }
    }

    layout
}

fn add_field(map: &mut BTreeMap<u8, u16>, report_id: u8, report_size: u32, report_count: u32) {
    let bits = report_size.saturating_mul(report_count);
    let bytes = bits.div_ceil(8).min(u16::MAX as u32) as u16;
    if bytes > 0 {
        *map.entry(report_id).or_insert(0) += bytes;
    }
}
