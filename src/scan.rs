//! Enumerate MCHOSE HID interfaces and query their battery.

use std::collections::HashSet;
use std::time::SystemTime;

use anyhow::Result;
use hidapi::{DeviceInfo, HidApi, HidDevice};

use crate::protocol;
use crate::types::{
    clean_product_name, is_dongle_pid, model_name_for, product_looks_like_dock,
    product_looks_like_keyboard, product_looks_wireless, Connection, Device, Reading,
};

#[derive(Clone, Debug)]
pub struct InterfaceReport {
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface: i32,
    pub usage_page: u16,
    pub usage: u16,
    pub manufacturer: String,
    pub product: String,
    pub path: String,
    pub vendor_page: bool,
    pub result: std::result::Result<String, String>,
}

#[derive(Clone, Debug)]
pub struct ScanReport {
    pub devices: Vec<Device>,
    pub interfaces: Vec<InterfaceReport>,
    pub fatal: Option<String>,
    pub permission_denied: bool,
    pub scanned_at: SystemTime,
}

impl ScanReport {
    fn fatal(message: impl Into<String>) -> Self {
        Self {
            devices: Vec::new(),
            interfaces: Vec::new(),
            fatal: Some(message.into()),
            permission_denied: false,
            scanned_at: SystemTime::now(),
        }
    }
}

#[derive(Clone, Debug)]
struct Candidate {
    vendor_id: u16,
    product_id: u16,
    interface: i32,
    usage_page: u16,
    usage: u16,
    manufacturer: String,
    product: String,
    path: String,
    serial: Option<String>,
    vendor_page: bool,
    is_dock: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProtocolKind {
    NewMouse,
    Native11,
    Hub13,
    Hub06,
    A5,
    A7,
    Gmouse,
}

impl ProtocolKind {
    fn name(self) -> &'static str {
        match self {
            ProtocolKind::NewMouse => "HUB 0x0900",
            ProtocolKind::Native11 => "Native 0x11",
            ProtocolKind::Hub13 => "Hub 0x13",
            ProtocolKind::Hub06 => "Hub 0x06",
            ProtocolKind::A5 => "A5/AX5",
            ProtocolKind::A7 => "A7 legacy",
            ProtocolKind::Gmouse => "Gmouse",
        }
    }

    fn run(self, device: &HidDevice, info: &DeviceInfo) -> Result<Reading> {
        match self {
            ProtocolKind::NewMouse => protocol::newmouse::read(device, info),
            ProtocolKind::Native11 => protocol::native11::read(device, info),
            ProtocolKind::Hub13 => {
                protocol::hub::read_kind(device, info, protocol::ReportKind::Wireless13)
            }
            ProtocolKind::Hub06 => {
                protocol::hub::read_kind(device, info, protocol::ReportKind::Wired06)
            }
            ProtocolKind::A5 => protocol::a5::read(device, info),
            ProtocolKind::A7 => protocol::a7::read(device, info),
            ProtocolKind::Gmouse => protocol::gmouse::read(device, info),
        }
    }
}

pub fn scan_devices(_debug: bool) -> ScanReport {
    let api = match HidApi::new() {
        Ok(api) => api,
        Err(err) => {
            return ScanReport::fatal(format!("无法初始化 HID 子系统：{err:#}"));
        }
    };

    let mut candidates = Vec::new();
    let mut seen_paths = HashSet::new();

    for info in api.device_list() {
        if !looks_like_mchose(info) {
            continue;
        }

        let product = info.product_string().unwrap_or("").to_string();
        if product_looks_like_keyboard(&product) {
            continue;
        }

        let vendor_page = info.usage_page() >= 0xff00;
        let interface = info.interface_number();
        let plausible = vendor_page
            || (info.vendor_id() == 0x2023 && interface == 2)
            || (info.vendor_id() == 0x5253 && info.product_id() == 0x1021)
            || (info.vendor_id() == 0x3837 && interface >= 2)
            || (info.vendor_id() == 0x41e4 && interface >= 1);

        if !plausible {
            continue;
        }

        let path = info.path().to_string_lossy().to_string();
        if !seen_paths.insert(path.clone()) {
            continue;
        }

        candidates.push(Candidate {
            vendor_id: info.vendor_id(),
            product_id: info.product_id(),
            interface,
            usage_page: info.usage_page(),
            usage: info.usage(),
            manufacturer: info.manufacturer_string().unwrap_or("").to_string(),
            product: product.clone(),
            path,
            serial: info.serial_number().map(|s| s.to_string()),
            vendor_page,
            is_dock: product_looks_like_dock(&product),
        });
    }

    // Vendor-defined collections first, then the highest interface number:
    // MCHOSE puts the battery channel on the last vendor interface.
    candidates.sort_by_key(|c| (!c.vendor_page, -c.interface, c.vendor_id, c.product_id));

    let has_non_dock = candidates.iter().any(|c| !c.is_dock);
    let primary_hint = primary_name_hint(&candidates);
    let mut devices = Vec::new();
    let mut interfaces = Vec::new();
    let mut permission_denied = false;

    for candidate in &candidates {
        if candidate.is_dock && has_non_dock {
            interfaces.push(interface_report(
                candidate,
                Err("MCHOSE 充电底座（不是鼠标电量接口），已跳过".to_string()),
            ));
            continue;
        }

        let Some(info) = api
            .device_list()
            .find(|info| info.path().to_string_lossy() == candidate.path)
        else {
            interfaces.push(interface_report(
                candidate,
                Err("设备在扫描过程中断开".to_string()),
            ));
            continue;
        };

        let device = match info.open_device(&api) {
            Ok(device) => device,
            Err(err) => {
                let message = format!("打开 HID 接口失败：{err}");
                if is_permission_error(&message) {
                    permission_denied = true;
                }
                interfaces.push(interface_report(candidate, Err(message)));
                continue;
            }
        };

        let path = info.path().to_string_lossy();
        let layout = crate::hid_layout::read_layout(&device, Some(&path));

        let mut errors = Vec::new();
        let mut success = None;
        for kind in protocol_order(info, layout.as_ref()) {
            match kind.run(&device, info) {
                Ok(mut reading) => {
                    reading.protocol = kind.name().to_string();
                    success = Some(reading);
                    break;
                }
                Err(err) => errors.push(format!("{}: {err:#}", kind.name())),
            }
        }

        match success {
            Some(reading) => {
                let summary = format!(
                    "{}：{}% {} [{}]",
                    reading.protocol,
                    reading.percent,
                    reading.charge.label_zh(),
                    reading.connection.label_zh()
                );
                interfaces.push(interface_report(candidate, Ok(summary)));
                devices.push(device_from(candidate, reading));
            }
            None => {
                let message = errors.join(" | ");
                if is_permission_error(&message) {
                    permission_denied = true;
                }
                interfaces.push(interface_report(candidate, Err(message)));
            }
        }
    }

    rename_wireless(&mut devices, primary_hint.as_deref());
    let devices = merge_wired_and_wireless(devices);

    ScanReport {
        devices,
        interfaces,
        fatal: None,
        permission_denied,
        scanned_at: SystemTime::now(),
    }
}

fn looks_like_mchose(info: &DeviceInfo) -> bool {
    let manufacturer = info
        .manufacturer_string()
        .unwrap_or("")
        .to_ascii_lowercase();
    let product = info.product_string().unwrap_or("").to_ascii_lowercase();
    let text = format!("{manufacturer} {product}");
    text.contains("mchose")
        || text.contains("maicong")
        || text.contains("迈从")
        || matches!(
            info.vendor_id(),
            0x3837 | 0x41e4 | 0x2023 | 0x5253 | 0xa8a4 | 0xa8a5
        )
}

fn interface_report(
    candidate: &Candidate,
    result: std::result::Result<String, String>,
) -> InterfaceReport {
    InterfaceReport {
        vendor_id: candidate.vendor_id,
        product_id: candidate.product_id,
        interface: candidate.interface,
        usage_page: candidate.usage_page,
        usage: candidate.usage,
        manufacturer: candidate.manufacturer.clone(),
        product: candidate.product.clone(),
        path: candidate.path.clone(),
        vendor_page: candidate.vendor_page,
        result,
    }
}

fn is_known_gmouse(vid: u16, pid: u16) -> bool {
    matches!(
        (vid, pid),
        (0xa8a5, 0x2255)
            | (0x3837, 0x4011)
            | (0x3837, 0x4012)
            | (0x3837, 0x4013)
            | (0x3837, 0x4042)
            | (0x3837, 0x418c)
            | (0x3837, 0x4033)
    )
}

fn protocol_order(
    info: &DeviceInfo,
    layout: Option<&crate::hid_layout::ReportLayout>,
) -> Vec<ProtocolKind> {
    let vid = info.vendor_id();
    let pid = info.product_id();
    let page = info.usage_page();
    let product = info.product_string().unwrap_or("");
    let receiver = product_looks_wireless(product) || vid == 0x41e4 || is_dongle_pid(vid, pid);

    let mut order = Vec::new();
    let mut add = |kind: ProtocolKind| {
        if !order.contains(&kind) {
            order.push(kind);
        }
    };

    // Fixed legacy families.
    if vid == 0x2023 {
        add(ProtocolKind::A5);
    }
    if vid == 0x5253 && pid == 0x1021 {
        add(ProtocolKind::A7);
    }
    if page == 0xff01 {
        add(ProtocolKind::Gmouse);
    }

    if let Some(layout) = layout {
        let has_4d = layout.output_size(0x4d).is_some();
        let has_11 = layout.feature_size(0x11).is_some() || layout.output_size(0x11).is_some();
        let has_13 = layout.feature_size(0x13).is_some() || layout.output_size(0x13).is_some();
        // Some receiver descriptors advertise absurd 0x06 feature lengths;
        // ignore those so we do not issue a huge GET_FEATURE.
        let has_06 = layout
            .feature_size(0x06)
            .map(|size| size <= 256)
            .unwrap_or(false)
            || layout.output_size(0x06).is_some();

        if has_4d {
            // A5 V3 Ultra+ and current wired mice: 0x4D / command 0x0900.
            add(ProtocolKind::NewMouse);
        } else {
            if has_11 {
                add(ProtocolKind::Native11);
            }
            if has_13 {
                add(ProtocolKind::Hub13);
            }
            if has_06 {
                add(ProtocolKind::Hub06);
            }
            if has_11 {
                add(ProtocolKind::A7);
            }
            if is_known_gmouse(vid, pid) {
                add(ProtocolKind::Gmouse);
            }
        }
    } else {
        // Descriptor unavailable: conservative fallback for older devices.
        if vid == 0x3837 {
            add(ProtocolKind::NewMouse);
        }
        if receiver {
            add(ProtocolKind::Hub13);
            add(ProtocolKind::Native11);
            add(ProtocolKind::Hub06);
            add(ProtocolKind::A7);
        } else {
            add(ProtocolKind::Native11);
            add(ProtocolKind::Hub13);
            add(ProtocolKind::Hub06);
            add(ProtocolKind::A7);
        }
        if is_known_gmouse(vid, pid) {
            add(ProtocolKind::Gmouse);
        }
    }

    order
}

fn device_from(candidate: &Candidate, reading: Reading) -> Device {
    let mut name = clean_product_name(&candidate.product);
    if product_looks_wireless(&name) {
        if let (Some(vid), Some(pid)) = (reading.mouse_vid, reading.mouse_pid) {
            if let Some(model) = model_name_for(vid, pid) {
                name = model.to_string();
            }
        }
    }

    let key = format!(
        "dev:{:04x}:{:04x}:{}:{}",
        candidate.vendor_id,
        candidate.product_id,
        candidate.interface,
        candidate
            .serial
            .clone()
            .unwrap_or_else(|| candidate.path.clone())
    );

    let mut connections = vec![reading.connection];
    connections.sort_by_key(|connection| connection.sort_rank());
    connections.dedup();

    Device {
        key,
        name,
        vendor_id: candidate.vendor_id,
        product_id: candidate.product_id,
        interface: candidate.interface,
        path: candidate.path.clone(),
        serial: candidate.serial.clone(),
        connections,

        reading: Some(reading),
        error: None,
        updated_at: Some(SystemTime::now()),
    }
}

/// If exactly one wired MCHOSE mouse candidate exists, use its product name for
/// the matching 2.4G receiver. This works even when the wired battery query
/// fails (for example, a MagDock pass-through), so the receiver is still shown
/// as "MCHOSE A5 V3 Ultra+" instead of "MCHOSE 2.4G Wireless".
fn primary_name_hint(candidates: &[Candidate]) -> Option<String> {
    let mut names: Vec<String> = candidates
        .iter()
        .filter(|candidate| !candidate.is_dock && !product_looks_wireless(&candidate.product))
        .map(|candidate| clean_product_name(&candidate.product))
        .filter(|name| !name.eq_ignore_ascii_case("MCHOSE Mouse"))
        .collect();
    names.sort();
    names.dedup();
    if names.len() == 1 {
        names.pop()
    } else {
        None
    }
}

fn rename_wireless(devices: &mut [Device], hint: Option<&str>) {
    let primary = hint.map(str::to_string).or_else(|| {
        let primaries: Vec<String> = devices
            .iter()
            .filter(|device| !product_looks_wireless(&device.name) && device.reading.is_some())
            .map(|device| device.name.clone())
            .collect();
        if primaries.len() == 1 {
            Some(primaries[0].clone())
        } else {
            None
        }
    });
    let Some(primary) = primary else {
        return;
    };

    for device in devices.iter_mut() {
        if !product_looks_wireless(&device.name) {
            continue;
        }
        device.name = primary.clone();
        if let Some(reading) = &mut device.reading {
            reading.connection = Connection::Wireless24G;
        }
        if !device.connections.contains(&Connection::Wireless24G) {
            device.connections.push(Connection::Wireless24G);
            device
                .connections
                .sort_by_key(|connection| connection.sort_rank());
        }
    }
}

#[cfg(test)]
fn rename_wireless_from_primary(devices: &mut [Device]) {
    rename_wireless(devices, None);
}

fn has_connection(device: &Device, connection: Connection) -> bool {
    device.connections.contains(&connection)
}

fn can_merge(a: &Device, b: &Device) -> bool {
    let a_wired = has_connection(a, Connection::Wired);
    let a_wireless = has_connection(a, Connection::Wireless24G);
    let b_wired = has_connection(b, Connection::Wired);
    let b_wireless = has_connection(b, Connection::Wireless24G);

    (a_wired && b_wireless && !a_wireless) || (a_wireless && b_wired && !b_wireless)
}

fn merge_one(existing: &mut Device, incoming: Device) {
    let incoming_wired = has_connection(&incoming, Connection::Wired);
    let (mut base, other) = if incoming_wired {
        (incoming, existing.clone())
    } else {
        (existing.clone(), incoming)
    };

    let other_primary = other.primary_connection();
    base.connections.extend(other.connections);
    base.connections.push(other_primary);
    base.connections
        .sort_by_key(|connection| connection.sort_rank());
    base.connections.dedup();

    if base.reading.is_none() {
        base.reading = other.reading;
    }
    if base.error.is_none() {
        base.error = other.error;
    }
    base.updated_at = match (base.updated_at, other.updated_at) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (a, b) => a.or(b),
    };
    if base.reading.is_some() {
        base.error = None;
    }

    // The merged entry survives a switch between USB and 2.4G.
    base.key = format!("model:{}", base.name.to_ascii_lowercase());
    *existing = base;
}

fn merge_wired_and_wireless(devices: Vec<Device>) -> Vec<Device> {
    let mut merged: Vec<Device> = Vec::new();

    for device in devices {
        let position = merged.iter().position(|existing| {
            existing.name.eq_ignore_ascii_case(&device.name) && can_merge(existing, &device)
        });

        match position {
            Some(index) => merge_one(&mut merged[index], device),
            None => merged.push(device),
        }
    }

    merged.sort_by_key(|device| {
        (
            device.primary_connection().sort_rank(),
            device.name.to_ascii_lowercase(),
        )
    });
    merged
}

fn is_permission_error(message: &str) -> bool {
    let message = message.to_ascii_lowercase();
    message.contains("permission denied")
        || message.contains("access is denied")
        || message.contains("access denied")
        || message.contains("insufficient permission")
        || message.contains("os error 13")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ChargeState, Reading};

    fn fake_device(name: &str, connection: Connection, percent: u8) -> Device {
        let mut reading = Reading::new(percent, ChargeState::NotCharging, connection, "test");
        reading.mouse_vid = Some(0x3837);
        reading.mouse_pid = Some(0x1018);
        Device {
            key: format!("{name}:{connection:?}"),
            name: name.to_string(),
            vendor_id: 0x3837,
            product_id: if connection == Connection::Wired {
                0x1018
            } else {
                0x2004
            },
            interface: 2,
            path: "/dev/test".to_string(),
            serial: None,
            connections: vec![connection],
            reading: Some(reading),
            error: None,
            updated_at: Some(SystemTime::now()),
        }
    }

    #[test]
    fn merges_wired_and_wireless_with_same_model() {
        let wired = fake_device("MCHOSE A5 V3 Ultra+", Connection::Wired, 85);
        let wireless = fake_device("MCHOSE 2.4G Wireless", Connection::Wireless24G, 84);
        let mut devices = vec![wired, wireless];

        rename_wireless_from_primary(&mut devices);
        assert_eq!(devices[1].name, "MCHOSE A5 V3 Ultra+");

        let merged = merge_wired_and_wireless(devices);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].connections.contains(&Connection::Wired));
        assert!(merged[0].connections.contains(&Connection::Wireless24G));
        assert_eq!(merged[0].reading.as_ref().unwrap().percent, 85);
    }

    #[test]
    fn does_not_merge_two_wired_devices() {
        let first = fake_device("MCHOSE A5 V3 Ultra+", Connection::Wired, 85);
        let second = fake_device("MCHOSE A5 V3 Ultra+", Connection::Wired, 80);
        assert_eq!(merge_wired_and_wireless(vec![first, second]).len(), 2);
    }

    #[test]
    fn does_not_merge_two_wireless_devices() {
        let first = fake_device("MCHOSE 2.4G Wireless", Connection::Wireless24G, 85);
        let second = fake_device("MCHOSE 2.4G Wireless", Connection::Wireless24G, 80);
        assert_eq!(merge_wired_and_wireless(vec![first, second]).len(), 2);
    }
}
