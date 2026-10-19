# 迈从鼠标电量（MCHOSE Battery）

在 **Windows 任务栏** 和 **KDE Plasma 系统托盘** 显示迈从（MCHOSE）鼠标电量的小工具。

> 仓库：<https://github.com/QPRS-6/mchose_battery>


纯 Rust、单文件可执行程序，直接通过 HID 厂商协议读取电量，不需要安装官方 M HUB 驱动，也不依赖 GTK / AppIndicator。Linux 侧使用 KDE 原生 StatusNotifierItem（SNI），Windows 侧使用原生任务栏托盘。

![托盘图标预览](docs/tray-icon-preview.png)

> 图标从左到右：低电量、20%、55%、85% 充电中、100%、未知、休眠/上次电量。

## 功能

- 任务栏 / 系统托盘直接显示电量百分比；低于 20% 变红，充电时黄色边框 + 闪电。
- 工具提示显示型号、电量、充电状态、连接方式（USB / 2.4G）和使用的协议。
- 右键菜单：设备列表、立即刷新、重新扫描、开机启动、退出。
- 多设备：左键循环切换，菜单里可直接选择。
- USB 与 2.4G 同时在线时自动合并成一个设备，显示 `USB+2.4G`。
- 鼠标休眠/短暂离线时保留上次电量；10 分钟后标注“上次”，15 分钟后丢弃。
- 自动适配多代迈从协议（见下表），无需手动选择型号。
- `--scan` 可打印每个 HID 接口和协议的探测结果，方便反馈新型号。
- `--json` 可给 waybar / 脚本使用。

## 已适配协议

| 协议 | 典型型号 | 说明 |
|------|----------|------|
| **HUB 0x0900** | A5 V3 Ultra+、当前 0x3837 有线鼠标 | output report `0x4D` + command `0x0900`；电量 `n[12]`、充电 `n[11]`；另有 `0x0600` 主动上报 |
| **Native 0x11** | A7 V2 Ultra、L7 Ultra、部分旧 0x3837 鼠标 | report `0x11`，数据 XOR `0xFF` |
| **Hub 0x13 / Hub 0x06** | A5 Ultra / V2、A7 V2+、AX5 V2、G3 Ultra、K7/M7/L7、41e4 2.4G 接收器 | 官方 Web HUB 协议，`command=10 type=5` 或 `13 4a …`/`4a` 应答 |
| **A5/AX5 legacy** | A5 Pro、A5 Pro Max、AX5 Pro 等 | VID `0x2023`，65 字节 feature report，命令 `0x83` |
| **A7 legacy** | 旧 A7 | VID `0x5253:0x1021`，report `0x11` + XOR |
| **Gmouse** | G3、G3 V2、G3 V2 Pro、A7e、G7 等 | usage page `0xFF01`，`55 30 a5 …` → `aa 30` |

说明：

- “已适配”表示协议已实现并做了合成报文单元测试；不同固件可能存在差异，欢迎用 `--scan` 输出反馈。
- `MCHOSE MagDock` 是充电底座，不是鼠标电量接口，程序会自动跳过它；请通过鼠标的 USB 数据线或 2.4G 接收器读取。
- 不依赖官方 M HUB；如果两者同时运行且官方软件独占 HID，电量读取可能失败，退出 M HUB 即可。

## 系统要求

- **Windows 10 / 11**（x64）。
- **Arch Linux / CachyOS + KDE Plasma 5/6**（Wayland 或 X11）。其他 freedesktop 桌面只要支持 StatusNotifierItem 也可以。
- Linux 需要 `systemd-libs`（libudev）和可用的 D-Bus 会话。
- 构建需要 Rust 1.90+。

## Arch / KDE 安装

```bash
# 1. 安装构建依赖（如果还没有 Rust）
sudo pacman -S --needed rust cargo systemd-libs

# 2. 编译
cd /path/to/battery
cargo build --release

# 3. 安装二进制 + udev 权限规则 + 应用菜单项（会提示输入 sudo 密码）
sudo packaging/arch/install.sh

# 4. 重新插拔鼠标的 2.4G 接收器 / 数据线（让 uaccess ACL 生效）
#    然后先做一次探测：
mchose-battery --scan
```

`--scan` 正常时会输出类似：

```text
MCHOSE A5 V3 Ultra+(3955)：85%  充电中  [USB+2.4G]  协议 HUB 0x0900

HID 接口探测详情：
[1] 3837:1018  interface=2  usage=ff0b:00c4  vendor_page=true
    MCHOSE | MCHOSE A5 V3 Ultra+(3955)
    -> HUB 0x0900：85% 充电中 [USB]
[2] 41e4:2004  interface=1  usage=ff02:0002  vendor_page=true
    SINOWEALTH | MCHOSE 2.4G Wireless
    -> Hub 0x13：85% 充电中 [2.4G]
```

然后启动托盘：

```bash
mchose-battery
```

在托盘菜单里勾选 **开机启动** 即可写入 `~/.config/autostart/mchose-battery.desktop`。

如果 KDE 面板没有显示图标，请右键面板 → **配置面板** → **系统托盘** → **条目**，把 **MCHOSE Battery** 设为“始终显示”。

也可以直接使用 GitHub Actions 的 `mchose-battery-linux-x86_64` 压缩包：解压后执行 `sudo bash install.sh`（压缩包里已经带好二进制、udev 规则和菜单项）。

### 用 PKGBUILD 安装（可选）

```bash
cd packaging/arch
makepkg -si
```

PKGBUILD 会从当前源码目录构建，安装到 `/usr/bin/mchose-battery`，并把 udev 规则放到 `/usr/lib/udev/rules.d/`。

### 手动安装 udev 规则

如果不想运行安装脚本，至少需要让当前用户能读写鼠标的 hidraw 节点：

```bash
sudo install -Dm644 packaging/arch/90-mchose-battery.rules \
  /etc/udev/rules.d/90-mchose-battery.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
# 建议重新插拔接收器
```

检查权限：

```bash
getfacl /dev/hidraw* | grep -A2 '^# file'
```

当前用户应该能看到 `user:你的用户名:rw-` 的 ACL。若仍是 `root:root 600`，重新登录或重新插拔设备。

> 规则文件是 `90-`，排在 systemd 的 `73-seat-late.rules` 之后；因此规则里除了
> `TAG+="uaccess"` 还显式写了 `RUN{builtin}+="uaccess"`。如果你手工改过旧规则，
> 请重新安装项目里的最新规则。

临时验证（不依赖 udev 规则，重启/重插后会失效）：

```bash
sudo setfacl -m u:$USER:rw /dev/hidraw5 /dev/hidraw2
mchose-battery --scan
```

把 `/dev/hidrawN` 换成你鼠标实际的两个节点即可。

## Windows 安装

### 方式 A：GitHub Actions 构建产物（推荐给没有 Rust 的 Windows）

把仓库推到 GitHub 后，Actions 会自动构建 `mchose-battery-windows-x86_64` 压缩包（包含 exe、install.ps1、uninstall.ps1）。下载解压后，在该目录运行：

```powershell
powershell -ExecutionPolicy Bypass -File .\install.ps1
```

脚本会：

1. 复制到 `%LOCALAPPDATA%\MCHOSE Battery\mchose-battery.exe`；
2. 写入 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` 实现开机启动；
3. 创建开始菜单快捷方式并启动程序。

卸载：

```powershell
powershell -ExecutionPolicy Bypass -File packaging\windows\uninstall.ps1
```

### 方式 B：在 Windows 上自行编译

安装 [Rust MSVC 工具链](https://rustup.rs/)，然后：

```powershell
cargo build --release
powershell -ExecutionPolicy Bypass -File packaging\windows\install.ps1
```

产物：`target\release\mchose-battery.exe`。它是 GUI 子系统程序，双击不会弹黑框；`--scan` / `--json` 会自动附加或创建控制台。

Windows 11 默认会把新托盘图标放进 `^` 隐藏区。点击任务栏的 `^`，把 **MCHOSE Battery** 拖到任务栏，或在 **设置 → 个性化 → 任务栏 → 其他系统托盘图标** 中把它设为“开”。

## 使用

```text
mchose-battery                 启动托盘（默认）
mchose-battery --scan          打印 HID 接口 / 协议探测详情（排查新型号）
mchose-battery --once          读取一次并退出
mchose-battery --json          输出 JSON（自动按 --once 处理）
mchose-battery --interval 60   轮询间隔秒数，默认 60，最小 10
mchose-battery --help
mchose-battery --version
```

`--json` 示例：

```json
{
  "scanned_at_unix": 1792381613,
  "fatal": null,
  "permission_denied": false,
  "devices": [
    {
      "name": "MCHOSE A5 V3 Ultra+(3955)",
      "vendor_id": 14391,
      "product_id": 4120,
      "interface": 2,
      "path": "/dev/hidraw5",
      "connections": ["wired", "wireless_2_4g"],
      "reading": {
        "percent": 85,
        "charge": "charging",
        "connection": "wired",
        "protocol": "HUB 0x0900",
        "mouse_vid": 14391,
        "mouse_pid": 4120
      }
    }
  ]
}
```

### 托盘交互

- **左键**：一个设备时立即刷新；多个设备时循环切换。
- **右键**：打开菜单。
- 菜单中的设备行显示 `型号 电量 充电状态 [连接方式]`，点击可选中为主显示设备。
- **立即刷新 / 重新扫描设备**：立即触发一次 HID 扫描。
- **开机启动**：Linux 写入 XDG autostart，Windows 写入 HKCU Run。
- **退出**：关闭托盘程序。

## 工作原理

程序不做驱动注入，也不读取官方驱动的缓存，而是直接对 HID 厂商接口收发 feature/output report：

1. 枚举 HID 设备，按 VID/PID、产品名和 usage page 筛选迈从相关接口；
2. 读取 HID 报告描述符，只尝试该接口实际存在的 report（HUB 0x0900 → Native 0x11 → Hub 0x13/0x06 → 旧 A5/A7 → Gmouse），避免向不存在的 report 发请求导致阻塞；
3. 解析电量与充电状态，失败时保留上一次成功值；
4. 默认每 60 秒查询一次；鼠标休眠时查询失败不会清空显示，10 分钟后标注“上次”，15 分钟后丢弃；
5. USB 与 2.4G 同时上报时合并显示，优先使用有线读数。

A5 V3 Ultra+ 的接口 2 报告描述符包含：

```text
Report ID 0x11: Input 20 B, Feature 20 B, Output 20 B
Report ID 0x13: Input 20 B, Feature 20 B, Output 20 B
Report ID 0x4D: Input/Feature/Output 63 B
```

A5 V3 Ultra+ 实际使用 **HUB 0x0900**：

```text
output 0x4D (64 B): 4D 01 01 00 00 09 00 00 08 00 ...
                                  ^^ ^^    ^
                                  |  |     XOR checksum
                                  |  command 0x0900
                                  data length 0
input 0x4D: ... 00 09 ... <vid le16> <pid le16> ... n[10] n[11] n[12]
                                                          |     |     |
                                                          |     |     battery %
                                                          |     charge status
                                                          connect status
```

设备还会用 `0x0600` 主动上报电量和充电状态（带 flag 位）。鼠标放在底座/关机时可能返回 `0x0900` 错误包（`... ff 00...`），程序会丢弃它而不是显示 0%。

Native 0x11 查询：`11 f9 ff ff ...`（`0x06 ^ 0xFF = 0xF9`，其余字节 `0xFF`），等待设备应答后 GET_FEATURE，再把 payload 与 `0xFF` 异或，得到：

```text
command, vid(le16), pid(le16), firmware(le32), flags, battery, charge
```

`flags & 0x07`：`0=USB`、`1=2.4G`、`2=蓝牙`、`6=休眠`、`7=关机`。

Hub 协议的无线包为 `13 4a 01 …`，有线包为 `06 87 00 00 01 00 02 00 …`，应答里查找 `command=10, type=5`，随后是电量与状态 nibble。

## 常见问题

### `Permission denied` / 没有权限访问 hidraw

Linux 下普通用户默认不能打开 `/dev/hidraw*`。安装 udev 规则并重新插拔设备：

```bash
sudo packaging/arch/install.sh
# 或手动：
sudo install -Dm644 packaging/arch/90-mchose-battery.rules /etc/udev/rules.d/
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=hidraw
```

如果仍然不行，注销重新登录，或检查：

```bash
getfacl /dev/hidraw5
```

### 未发现设备

- 确认鼠标已开机；2.4G 接收器插在 USB 口上，或鼠标通过数据线连接。
- 关闭官方 **M HUB** / 旧驱动，避免 HID 被独占。
- 运行 `mchose-battery --scan` 查看是否枚举到了接口。
- 某些 USB Hub / KVM 可能不转发厂商 HID 报告，直连主板 USB 口试试。
- 如果所有协议都失败，运行 `MCHOSE_DEBUG_RAW=1 mchose-battery --scan`，把带 `[raw ...]` 的输出发出来即可定位新型号协议。
- MagDock 充电底座本身不提供鼠标电量接口；请用数据线或 2.4G 接收器。

### 电量不更新 / 显示“上次”

- 无线鼠标休眠时不会回应查询，移动一下鼠标或按一下按键唤醒，等待下一轮轮询（默认 60 秒）。
- 鼠标放在 MagDock 上但底部电源关闭时，底座通道可能只返回错误包；把鼠标拿下来开机、或直接用数据线连接鼠标再扫描。
- 程序会保留最后读数 15 分钟；10 分钟后图标变成灰色并在菜单/提示中标注“上次”。
- 想更快刷新：`mchose-battery --interval 10`，或右键“立即刷新”。

### Windows 上读取失败

- 先退出官方 M HUB，再重启本程序。
- 少数主板/驱动会独占 HID；尝试换 USB 口，或以管理员身份运行一次排查。
- `--scan` 在 release 版会自动打开控制台；如果没有输出，请在 PowerShell 中运行并查看。

### 多个设备 / 两个条目

- 如果 USB 和 2.4G 都能读到电量，程序会自动合并（菜单显示 `[USB+2.4G]`）。
- 多只鼠标会分别列出；左键循环，菜单点击选择。
- 同名同型号的两只鼠标可能显示为两个条目，这是为了避免误合并。

## 开发

```bash
cargo test                     # 协议解析 + 图标渲染单元测试
cargo run -- --scan            # 真机探测（需要 udev 权限）
MCHOSE_DEBUG_RAW=1 cargo run -- --scan   # 同时打印每个 HID 原始应答
cargo run                      # 启动托盘
cargo run --example render_icons
magick target/icon-preview/*.ppm target/icon-preview/preview.png
```

目录：

```text
src/
  protocol/       各代迈从 HID 协议（native11 / hub / a5 / a7 / gmouse）
  scan.rs         设备枚举、协议自动探测、USB+2.4G 合并
  service.rs      后台轮询与休眠保活
  icon.rs         32×32 托盘图标绘制
  ui.rs           菜单 / 工具提示 / 图标（tray-icon 共用）
  platform/       linux.rs（KDE SNI）、windows.rs（Win32 托盘 + winit）
  autostart.rs    XDG autostart / Windows Run 键
packaging/
  arch/           udev 规则、install.sh、PKGBUILD、.desktop
  windows/         install.ps1 / uninstall.ps1
```

## 已知限制

- 仅支持 Windows 与 Linux/KDE；macOS 未适配。
- 各型号固件可能存在差异，无法保证所有迈从鼠标都能读取；`--scan` 输出是反馈问题的关键信息。
- 蓝牙模式是否可用取决于固件是否在 HID 报告中暴露电量；本项目主要针对 USB / 2.4G。
- 程序不修改鼠标设置（DPI、灯效、宏），只读取电量。
- 与 MCHOSE / 迈从官方无隶属关系；使用风险自负。

## License

MIT

