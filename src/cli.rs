//! Command-line parsing and the diagnostic `--scan` / `--json` modes.

use anyhow::{bail, Result};
use serde_json::json;

use crate::scan::{scan_devices, ScanReport};

#[derive(Clone, Debug)]
pub struct Args {
    pub scan: bool,
    pub json: bool,
    pub once: bool,
    pub interval: u64,
    pub help: bool,
    pub version: bool,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            scan: false,
            json: false,
            once: false,
            interval: 60,
            help: false,
            version: false,
        }
    }
}

pub fn parse<I: Iterator<Item = String>>(args: I) -> Result<Args> {
    let mut parsed = Args::default();
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => parsed.help = true,
            "-V" | "--version" => parsed.version = true,
            "--scan" => parsed.scan = true,
            "--json" => {
                parsed.json = true;
                parsed.once = true;
            }
            "--once" => parsed.once = true,
            "--interval" => {
                let value = args.next().unwrap_or_default();
                parsed.interval = parse_interval(&value)?;
            }
            value if value.starts_with("--interval=") => {
                parsed.interval = parse_interval(&value["--interval=".len()..])?;
            }
            other => bail!("未知参数：{other}（使用 --help 查看用法）"),
        }
    }

    Ok(parsed)
}

fn parse_interval(value: &str) -> Result<u64> {
    let seconds: u64 = value
        .parse()
        .map_err(|_| anyhow::anyhow!("--interval 需要一个整数秒数"))?;
    Ok(seconds.max(10))
}

pub fn print_help() {
    println!(
        "mchose-battery {}\n\
         在 Windows 任务栏 / KDE 系统托盘显示迈从（MCHOSE）鼠标电量。\n\n\
         用法：\n\
           mchose-battery                 启动托盘\n\
           mchose-battery --scan          扫描并打印 HID 接口/协议详情（排查用）\n\
           mchose-battery --once          读取一次并退出\n\
           mchose-battery --json          输出 JSON（供 waybar / 脚本使用）\n\
           mchose-battery --interval 60   轮询间隔秒数（默认 60，最小 10）\n\n\
         选项：\n\
           -h, --help                     显示帮助\n\
           -V, --version                  显示版本\n",
        env!("CARGO_PKG_VERSION")
    );
}

pub fn run_cli(args: Args) -> Result<()> {
    let report = scan_devices(args.scan);

    if args.json {
        let seconds = report
            .scanned_at
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let payload = json!({
            "scanned_at_unix": seconds,
            "fatal": report.fatal,
            "permission_denied": report.permission_denied,
            "devices": &report.devices,
        });
        println!("{}", serde_json::to_string_pretty(&payload)?);
        return Ok(());
    }

    print_summary(&report);
    if args.scan {
        print_interfaces(&report);
    }
    if report.permission_denied {
        print_permission_hint();
    }

    Ok(())
}

fn print_summary(report: &ScanReport) {
    if let Some(fatal) = &report.fatal {
        eprintln!("错误：{fatal}");
        return;
    }

    if report.devices.is_empty() {
        println!("未发现迈从鼠标。");
        println!("请确认鼠标已开机，2.4G 接收器或 USB 数据线已连接。");
        return;
    }

    for device in &report.devices {
        match &device.reading {
            Some(reading) => println!(
                "{}：{}%  {}  [{}]  协议 {}",
                device.name,
                reading.percent,
                reading.charge.label_zh(),
                device.connection_label(),
                reading.protocol
            ),
            None => println!("{}：读取失败", device.name),
        }
    }
}

fn print_interfaces(report: &ScanReport) {
    println!("\nHID 接口探测详情：");
    if report.interfaces.is_empty() {
        println!("  （没有发现 MCHOSE 相关的厂商 HID 接口）");
        return;
    }

    for (index, iface) in report.interfaces.iter().enumerate() {
        println!(
            "[{}] {:04x}:{:04x}  interface={}  usage={:04x}:{:04x}  vendor_page={}",
            index + 1,
            iface.vendor_id,
            iface.product_id,
            iface.interface,
            iface.usage_page,
            iface.usage,
            iface.vendor_page
        );
        println!("    {} | {}", iface.manufacturer, iface.product);
        println!("    path: {}", iface.path);
        match &iface.result {
            Ok(summary) => println!("    -> {summary}"),
            Err(error) => println!("    -> 失败：{error}"),
        }
    }
}

fn print_permission_hint() {
    eprintln!();
    eprintln!("没有权限访问 /dev/hidraw*。请安装 udev 规则后重新插拔接收器：");
    eprintln!(
        "  sudo install -Dm644 packaging/arch/90-mchose-battery.rules /etc/udev/rules.d/90-mchose-battery.rules"
    );
    eprintln!("  sudo udevadm control --reload-rules");
    eprintln!("  sudo udevadm trigger --subsystem-match=hidraw");
    eprintln!("（或者直接运行项目里的 packaging/arch/install.sh）");
}
