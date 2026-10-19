//! KDE Plasma / freedesktop tray implementation using StatusNotifierItem via
//! `tray-icon`'s `ksni` backend. No GTK or AppIndicator libraries are needed.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

use anyhow::{Context, Result};
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

use crate::autostart;
use crate::cli::Args;
use crate::scan::ScanReport;
use crate::service::BatteryService;
use crate::types::Device;
use crate::ui;

enum UiEvent {
    Scanned(ScanReport),
}

pub fn run(args: Args) -> Result<()> {
    let (tx, rx) = mpsc::channel();

    let mut devices: Vec<Device> = Vec::new();
    let mut selected = 0usize;
    let mut startup = autostart::is_enabled().unwrap_or(false);
    let mut notice: Option<String> = None;
    let mut dirty = true;

    let tray = TrayIconBuilder::new()
        .with_tooltip(ui::tooltip(&devices, selected, notice.as_deref()))
        .with_icon(ui::current_icon(&devices, selected))
        .with_menu(Box::new(ui::build_menu(&devices, selected, startup)))
        .with_menu_on_left_click(false)
        .with_menu_on_right_click(true)
        .build()
        .context(
            "无法创建系统托盘图标。请确认正在 KDE Plasma 桌面会话中运行，\
             并且系统托盘（StatusNotifierWatcher）可用。",
        )?;

    let service = BatteryService::spawn(Duration::from_secs(args.interval), {
        let tx = tx.clone();
        move |report| {
            let _ = tx.send(UiEvent::Scanned(report));
        }
    });

    loop {
        while let Ok(event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click {
                button,
                button_state,
                ..
            } = event
            {
                if button_state == MouseButtonState::Up && button == MouseButton::Left {
                    if devices.len() > 1 {
                        selected = (selected + 1) % devices.len();
                        dirty = true;
                    } else {
                        service.refresh();
                    }
                }
            }
        }

        while let Ok(event) = MenuEvent::receiver().try_recv() {
            let id = event.id.as_ref();
            match id {
                ui::MENU_REFRESH | ui::MENU_RESCAN => service.refresh(),
                ui::MENU_STARTUP => {
                    let target = !startup;
                    match autostart::set_enabled(target) {
                        Ok(()) => {
                            startup = target;
                            dirty = true;
                        }
                        Err(err) => eprintln!("更新开机启动失败：{err:#}"),
                    }
                }
                ui::MENU_QUIT => return Ok(()),
                _ => {
                    if let Some(index) = id.strip_prefix(ui::MENU_DEVICE_PREFIX) {
                        if let Ok(index) = index.parse::<usize>() {
                            if index < devices.len() {
                                selected = index;
                                dirty = true;
                            }
                        }
                    }
                }
            }
        }

        match rx.recv_timeout(Duration::from_millis(250)) {
            Ok(UiEvent::Scanned(report)) => {
                devices = report.devices;
                notice = report.fatal.clone().or_else(|| {
                    if report.permission_denied {
                        Some("无权限访问 HID：请安装 udev 规则".to_string())
                    } else {
                        None
                    }
                });
                if selected >= devices.len() {
                    selected = 0;
                }
                dirty = true;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        if dirty {
            tray.set_icon(Some(ui::current_icon(&devices, selected)))?;
            tray.set_tooltip(Some(ui::tooltip(&devices, selected, notice.as_deref())))?;
            tray.set_menu(Some(Box::new(ui::build_menu(&devices, selected, startup))));
            dirty = false;
        }
    }

    Ok(())
}
