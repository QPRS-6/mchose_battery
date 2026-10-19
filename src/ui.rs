//! Shared tray menu / tooltip / icon construction. Both the KDE
//! StatusNotifierItem backend and the Windows tray backend use `tray-icon`,
//! so this code is identical on both platforms.

use tray_icon::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tray_icon::Icon;

use crate::icon;
use crate::types::{ChargeState, Device};

pub const MENU_REFRESH: &str = "refresh";
pub const MENU_RESCAN: &str = "rescan";
pub const MENU_STARTUP: &str = "startup";
pub const MENU_QUIT: &str = "quit";
pub const MENU_DEVICE_PREFIX: &str = "device:";

pub fn current_device(devices: &[Device], selected: usize) -> Option<&Device> {
    if devices.is_empty() {
        None
    } else {
        devices.get(selected.min(devices.len() - 1))
    }
}

pub fn build_menu(devices: &[Device], selected: usize, startup_enabled: bool) -> Menu {
    let menu = Menu::new();

    let header = MenuItem::with_id("header", "MCHOSE Battery · 迈从鼠标电量", false, None);
    let _ = menu.append(&header);

    if devices.is_empty() {
        let empty = MenuItem::with_id("empty", "未发现迈从鼠标", false, None);
        let hint = MenuItem::with_id("hint", "请确认 2.4G 接收器或数据线已连接", false, None);
        let _ = menu.append(&empty);
        let _ = menu.append(&hint);
    } else {
        for (index, device) in devices.iter().enumerate() {
            let marker = if index == selected.min(devices.len() - 1) {
                "● "
            } else {
                "○ "
            };
            let item = MenuItem::with_id(
                format!("{MENU_DEVICE_PREFIX}{index}"),
                format!("{marker}{}", device.menu_label()),
                true,
                None,
            );
            let _ = menu.append(&item);
        }
    }

    let _ = menu.append(&PredefinedMenuItem::separator());
    let refresh = MenuItem::with_id(MENU_REFRESH, "立即刷新", true, None);
    let rescan = MenuItem::with_id(MENU_RESCAN, "重新扫描设备", true, None);
    let _ = menu.append(&refresh);
    let _ = menu.append(&rescan);

    let startup = CheckMenuItem::with_id(MENU_STARTUP, "开机启动", true, startup_enabled, None);
    let _ = menu.append(&startup);

    let _ = menu.append(&PredefinedMenuItem::separator());
    let quit = MenuItem::with_id(MENU_QUIT, "退出", true, None);
    let _ = menu.append(&quit);

    menu
}

pub fn tooltip(devices: &[Device], selected: usize, notice: Option<&str>) -> String {
    let Some(device) = current_device(devices, selected) else {
        let mut text = "迈从鼠标电量\n未发现设备\n请确认接收器或数据线已连接".to_string();
        if let Some(notice) = notice {
            text.push('\n');
            text.push_str(notice);
        }
        return text;
    };

    let mut text = device.tooltip();
    if let Some(notice) = notice {
        text.push('\n');
        text.push_str(notice);
    }
    if devices.len() > 1 {
        text.push_str(&format!("\n（共 {} 个设备，左键切换）", devices.len()));
    }
    text
}

pub fn current_icon(devices: &[Device], selected: usize) -> Icon {
    let device = current_device(devices, selected);
    let percent = device.and_then(|device| device.percent());
    let charge = device
        .map(|device| device.charge())
        .unwrap_or(ChargeState::Unknown);
    let connected = device
        .map(|device| device.reading.is_some())
        .unwrap_or(false);
    let stale = device.map(|device| device.is_stale()).unwrap_or(false);

    let rgba = icon::render_rgba(percent, charge, connected, stale);
    Icon::from_rgba(rgba, icon::SIZE, icon::SIZE).unwrap_or_else(|_| {
        Icon::from_rgba(
            vec![0u8; (icon::SIZE * icon::SIZE * 4) as usize],
            icon::SIZE,
            icon::SIZE,
        )
        .expect("32x32 transparent icon is valid")
    })
}
