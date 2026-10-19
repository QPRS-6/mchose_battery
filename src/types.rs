//! Shared data types used by the HID scanner and the tray UI.

use std::time::{Duration, SystemTime};

use serde::Serialize;

/// Charging state reported by a MCHOSE mouse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeState {
    #[default]
    Unknown,
    NotCharging,
    Charging,
    Full,
}

impl ChargeState {
    pub fn label_zh(self) -> &'static str {
        match self {
            ChargeState::Unknown => "未知",
            ChargeState::NotCharging => "未充电",
            ChargeState::Charging => "充电中",
            ChargeState::Full => "已充满",
        }
    }

    pub fn is_charging(self) -> bool {
        matches!(self, ChargeState::Charging)
    }

    pub fn is_full(self) -> bool {
        matches!(self, ChargeState::Full)
    }

    /// Compact label used in the menu, where horizontal space is limited.
    pub fn short_label(self) -> &'static str {
        match self {
            ChargeState::Unknown => "未知",
            ChargeState::NotCharging => "放电",
            ChargeState::Charging => "⚡充电",
            ChargeState::Full => "✓已满",
        }
    }
}

/// Physical link used to talk to the mouse.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Connection {
    Wired,
    #[serde(rename = "wireless_2_4g")]
    Wireless24G,
    Bluetooth,
    #[default]
    Unknown,
}

impl Connection {
    pub fn label_zh(self) -> &'static str {
        match self {
            Connection::Wired => "USB",
            Connection::Wireless24G => "2.4G",
            Connection::Bluetooth => "蓝牙",
            Connection::Unknown => "未知",
        }
    }

    pub(crate) fn sort_rank(self) -> u8 {
        match self {
            Connection::Wired => 0,
            Connection::Wireless24G => 1,
            Connection::Bluetooth => 2,
            Connection::Unknown => 3,
        }
    }
}

/// A successful battery query.
#[derive(Clone, Debug, Serialize)]
pub struct Reading {
    pub percent: u8,
    pub charge: ChargeState,
    pub connection: Connection,
    pub protocol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse_vid: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mouse_pid: Option<u16>,
    #[serde(skip)]
    pub read_at: SystemTime,
}

impl Reading {
    pub fn new(
        percent: u8,
        charge: ChargeState,
        connection: Connection,
        protocol: impl Into<String>,
    ) -> Self {
        Self {
            percent,
            charge,
            connection,
            protocol: protocol.into(),
            mouse_vid: None,
            mouse_pid: None,
            read_at: SystemTime::now(),
        }
    }
}

/// A MCHOSE device that the tray can display.
#[derive(Clone, Debug, Serialize)]
pub struct Device {
    /// Stable-ish identity used to keep the last value while the mouse sleeps.
    #[serde(skip)]
    pub key: String,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub interface: i32,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    /// All links that are known to reach this mouse (usually USB and/or 2.4G).
    pub connections: Vec<Connection>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reading: Option<Reading>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip)]
    pub updated_at: Option<SystemTime>,
}

impl Device {
    pub fn percent(&self) -> Option<u8> {
        self.reading.as_ref().map(|r| r.percent)
    }

    pub fn charge(&self) -> ChargeState {
        self.reading.as_ref().map(|r| r.charge).unwrap_or_default()
    }

    pub fn primary_connection(&self) -> Connection {
        self.connections
            .iter()
            .copied()
            .min_by_key(|c| c.sort_rank())
            .unwrap_or_default()
    }

    pub fn connection_label(&self) -> String {
        if self.connections.is_empty() {
            return "未知".to_string();
        }
        let mut connections = self.connections.clone();
        connections.sort_by_key(|connection| connection.sort_rank());
        connections.dedup();
        connections
            .iter()
            .map(|connection| connection.label_zh())
            .collect::<Vec<_>>()
            .join("+")
    }

    /// Stale means: we have a value, but it was not refreshed recently.
    pub fn is_stale(&self) -> bool {
        match (self.reading.as_ref(), self.updated_at) {
            (Some(_), Some(at)) => {
                SystemTime::now().duration_since(at).unwrap_or_default()
                    > Duration::from_secs(10 * 60)
            }
            (Some(_), None) => true,
            _ => false,
        }
    }

    pub fn menu_label(&self) -> String {
        match &self.reading {
            Some(reading) => format!(
                "{}  {}%  {}  [{}]{}",
                self.name,
                reading.percent,
                reading.charge.short_label(),
                self.connection_label(),
                if self.is_stale() { " · 上次" } else { "" }
            ),
            None => format!("{}  读取失败", self.name),
        }
    }

    pub fn tooltip(&self) -> String {
        match &self.reading {
            Some(reading) => format!(
                "{}：{}%\n{} · {}\n协议：{}{}",
                self.name,
                reading.percent,
                reading.charge.label_zh(),
                self.connection_label(),
                reading.protocol,
                if self.is_stale() {
                    "\n（设备休眠/离线，显示上次电量）"
                } else {
                    ""
                }
            ),
            None => format!(
                "{}：电量未知{}",
                self.name,
                self.error
                    .as_ref()
                    .map(|e| format!("\n{e}"))
                    .unwrap_or_default()
            ),
        }
    }
}

/// Trim duplicated vendor prefixes from HID product strings.
pub fn clean_product_name(raw: &str) -> String {
    let mut s = raw.trim();
    for prefix in ["SINOWEALTH ", "wch.cn "] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest;
        }
    }
    if let Some(rest) = s.strip_prefix("MCHOSE ") {
        if rest.starts_with("MCHOSE ") {
            s = rest;
        }
    }
    if s.is_empty() {
        "MCHOSE Mouse".to_string()
    } else {
        s.to_string()
    }
}

/// Model names for PID values seen in MCHOSE's web HUB and public projects.
pub fn model_name_for(vid: u16, pid: u16) -> Option<&'static str> {
    match (vid, pid) {
        (0x3837, 0x1018) => Some("MCHOSE A5 V3 Ultra+"),
        (0x3837, 0x100b) => Some("MCHOSE A7 V2 Ultra"),
        (0x3837, 0x4019) => Some("MCHOSE A7 V2 Ultra"),
        (0x5253, 0x1020) => Some("MCHOSE L7 Ultra"),
        (0x5253, 0x00b1) => Some("MCHOSE L7 Ultra"),
        (0x5253, 0x1021) => Some("MCHOSE A7"),
        (0xa8a5, 0x2255) => Some("MCHOSE G3 A"),
        (0x3837, 0x4011) | (0x3837, 0x4012) | (0x3837, 0x4013) => Some("MCHOSE A7e"),
        (0x3837, 0x4042) => Some("MCHOSE G3 V2 Pro"),
        (0x3837, 0x418c) => Some("MCHOSE G3 V2"),
        (0x3837, 0x4033) => Some("MCHOSE A7 V3 Ultra+"),
        _ => None,
    }
}

/// Known 2.4G receiver PIDs.
pub fn is_dongle_pid(vid: u16, pid: u16) -> bool {
    matches!(
        (vid, pid),
        (0x3837, 0x100b) | (0x5253, 0x1020) | (0x2023, 0xf013) | (0x2023, 0xf022)
    )
}

pub fn product_looks_wireless(product: &str) -> bool {
    let product = product.to_ascii_lowercase();
    product.contains("2.4g")
        || product.contains("2.4 g")
        || product.contains("wireless")
        || product.contains("receiver")
        || product.contains("dongle")
}

pub fn product_looks_like_keyboard(product: &str) -> bool {
    let product = product.to_ascii_lowercase();
    product.contains("keyboard")
        || product.contains("k99")
        || product.contains("k87")
        || product.contains("k75")
        || product.contains("k68")
        || product.contains("k61")
        || product.contains("k84")
        || product.contains("k96")
        || product.contains("k98")
        || product.contains("k100")
}

pub fn product_looks_like_dock(product: &str) -> bool {
    let product = product.to_ascii_lowercase();
    product.contains("magdock") || product.contains("dock")
}
