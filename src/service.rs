//! Background battery polling.
//!
//! HID queries can block for a couple of seconds (the native 0x11 protocol
//! waits for the mouse to answer), so the tray UI never talks to HID directly.

use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::scan::{scan_devices, ScanReport};
use crate::types::Device;

enum Request {
    Refresh,
    Stop,
}

pub struct BatteryService {
    tx: mpsc::Sender<Request>,
}

impl BatteryService {
    pub fn spawn<F>(interval: Duration, on_report: F) -> Self
    where
        F: Fn(ScanReport) + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || worker(interval, rx, on_report));
        Self { tx }
    }

    /// Ask the worker to scan again as soon as possible.
    pub fn refresh(&self) {
        let _ = self.tx.send(Request::Refresh);
    }
}

impl Drop for BatteryService {
    fn drop(&mut self) {
        let _ = self.tx.send(Request::Stop);
    }
}

fn worker<F>(interval: Duration, rx: mpsc::Receiver<Request>, on_report: F)
where
    F: Fn(ScanReport) + Send + 'static,
{
    let interval = if interval < Duration::from_secs(10) {
        Duration::from_secs(10)
    } else {
        interval
    };

    let mut last_devices: Vec<Device> = Vec::new();
    let mut next = Instant::now();

    loop {
        if Instant::now() >= next {
            let mut report = scan_devices(false);
            merge_stale_devices(&mut report, &last_devices);
            last_devices = report.devices.clone();
            on_report(report);
            next = Instant::now() + interval;
        }

        let wait = next
            .saturating_duration_since(Instant::now())
            .min(Duration::from_millis(500));

        match rx.recv_timeout(wait) {
            Ok(Request::Refresh) => next = Instant::now(),
            Ok(Request::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

/// Keep the last known percentage when a mouse goes to sleep or its interface
/// is temporarily busy. Values older than 15 minutes are dropped.
fn merge_stale_devices(report: &mut ScanReport, last_devices: &[Device]) {
    let now = SystemTime::now();
    let fatal = report.fatal.clone();

    for old in last_devices {
        if old.reading.is_none() {
            continue;
        }

        let already_present = report
            .devices
            .iter()
            .any(|device| device.key == old.key || device.name.eq_ignore_ascii_case(&old.name));
        if already_present {
            continue;
        }

        let recent = old
            .updated_at
            .map(|at| now.duration_since(at).unwrap_or_default() < Duration::from_secs(15 * 60))
            .unwrap_or(false);
        if !recent {
            continue;
        }

        let mut stale = old.clone();
        stale.error = Some(
            fatal
                .clone()
                .unwrap_or_else(|| "设备休眠或离线，显示上次电量".to_string()),
        );
        report.devices.push(stale);
    }

    report.devices.sort_by_key(|device| {
        (
            device.primary_connection().sort_rank(),
            device.name.to_ascii_lowercase(),
        )
    });
}
