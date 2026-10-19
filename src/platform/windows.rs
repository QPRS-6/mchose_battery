//! Windows notification-area implementation using `tray-icon`'s native Win32
//! backend and winit's message loop.

use std::time::Duration;

use anyhow::{Context, Result};
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::autostart;
use crate::cli::Args;
use crate::scan::ScanReport;
use crate::service::BatteryService;
use crate::types::Device;
use crate::ui;

#[derive(Debug)]
enum UserEvent {
    Scanned(ScanReport),
    Menu(String),
    TrayClick,
}

struct App {
    proxy: EventLoopProxy<UserEvent>,
    tray: Option<TrayIcon>,
    service: Option<BatteryService>,
    devices: Vec<Device>,
    selected: usize,
    startup: bool,
    notice: Option<String>,
    dirty: bool,
    interval: Duration,
}

impl App {
    fn new(proxy: EventLoopProxy<UserEvent>, interval: u64) -> Self {
        Self {
            proxy,
            tray: None,
            service: None,
            devices: Vec::new(),
            selected: 0,
            startup: autostart::is_enabled().unwrap_or(false),
            notice: None,
            dirty: true,
            interval: Duration::from_secs(interval),
        }
    }

    fn apply_ui(&mut self) -> Result<()> {
        let Some(tray) = &self.tray else {
            return Ok(());
        };
        tray.set_icon(Some(ui::current_icon(&self.devices, self.selected)))?;
        tray.set_tooltip(Some(ui::tooltip(
            &self.devices,
            self.selected,
            self.notice.as_deref(),
        )))?;
        tray.set_menu(Some(Box::new(ui::build_menu(
            &self.devices,
            self.selected,
            self.startup,
        ))));
        self.dirty = false;
        Ok(())
    }

    fn select(&mut self, index: usize) {
        if index < self.devices.len() {
            self.selected = index;
            self.dirty = true;
        }
    }

    fn handle_menu(&mut self, event_loop: &ActiveEventLoop, id: &str) {
        match id {
            ui::MENU_REFRESH | ui::MENU_RESCAN => {
                if let Some(service) = &self.service {
                    service.refresh();
                }
            }
            ui::MENU_STARTUP => {
                let target = !self.startup;
                match autostart::set_enabled(target) {
                    Ok(()) => {
                        self.startup = target;
                        self.dirty = true;
                    }
                    Err(err) => eprintln!("更新开机启动失败：{err:#}"),
                }
            }
            ui::MENU_QUIT => event_loop.exit(),
            _ => {
                if let Some(index) = id.strip_prefix(ui::MENU_DEVICE_PREFIX) {
                    if let Ok(index) = index.parse::<usize>() {
                        self.select(index);
                    }
                }
            }
        }
    }
}

impl ApplicationHandler<UserEvent> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        event_loop.set_control_flow(ControlFlow::Wait);
        if self.tray.is_some() {
            return;
        }

        let tray = TrayIconBuilder::new()
            .with_tooltip(ui::tooltip(
                &self.devices,
                self.selected,
                self.notice.as_deref(),
            ))
            .with_icon(ui::current_icon(&self.devices, self.selected))
            .with_menu(Box::new(ui::build_menu(
                &self.devices,
                self.selected,
                self.startup,
            )))
            .with_menu_on_left_click(false)
            .with_menu_on_right_click(true)
            .build();

        let tray = match tray {
            Ok(tray) => tray,
            Err(err) => {
                eprintln!("无法创建任务栏托盘图标：{err:#}");
                event_loop.exit();
                return;
            }
        };
        self.tray = Some(tray);

        let proxy = self.proxy.clone();
        let service = BatteryService::spawn(self.interval, move |report| {
            let _ = proxy.send_event(UserEvent::Scanned(report));
        });
        self.service = Some(service);

        if let Err(err) = self.apply_ui() {
            eprintln!("更新托盘图标失败：{err:#}");
            event_loop.exit();
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        match event {
            UserEvent::Scanned(report) => {
                self.devices = report.devices;
                self.notice = report.fatal.clone().or_else(|| {
                    if report.permission_denied {
                        Some("无权限访问 HID：请关闭 M HUB 或以管理员身份运行".to_string())
                    } else {
                        None
                    }
                });
                if self.selected >= self.devices.len() {
                    self.selected = 0;
                }
                self.dirty = true;
            }
            UserEvent::Menu(id) => self.handle_menu(event_loop, &id),
            UserEvent::TrayClick => {
                if self.devices.len() > 1 {
                    self.selected = (self.selected + 1) % self.devices.len();
                    self.dirty = true;
                } else if let Some(service) = &self.service {
                    service.refresh();
                }
            }
        }

        if self.dirty {
            if let Err(err) = self.apply_ui() {
                eprintln!("更新托盘图标失败：{err:#}");
                event_loop.exit();
            }
        }
    }
}

pub fn run(args: Args) -> Result<()> {
    let event_loop = EventLoop::<UserEvent>::with_user_event()
        .build()
        .context("无法创建 Windows 事件循环")?;

    let tray_proxy = event_loop.create_proxy();
    TrayIconEvent::set_event_handler(Some(move |event| {
        if let TrayIconEvent::Click {
            button,
            button_state,
            ..
        } = event
        {
            if button_state == MouseButtonState::Up && button == MouseButton::Left {
                let _ = tray_proxy.send_event(UserEvent::TrayClick);
            }
        }
    }));

    let menu_proxy = event_loop.create_proxy();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let _ = menu_proxy.send_event(UserEvent::Menu(event.id.as_ref().to_string()));
    }));

    let mut app = App::new(event_loop.create_proxy(), args.interval);
    event_loop.run_app(&mut app)?;
    Ok(())
}
