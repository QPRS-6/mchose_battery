#!/usr/bin/env bash
# Install the binary and the MCHOSE hidraw udev rule on Arch / KDE.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
PROJECT_ROOT=$(cd -- "$SCRIPT_DIR/../.." && pwd)
BIN=${1:-}

if [[ -z "$BIN" ]]; then
    if [[ -f "$SCRIPT_DIR/mchose-battery" ]]; then
        # Flat release archive: binary sitting next to this script.
        # It may have lost its executable bit while being unzipped.
        BIN="$SCRIPT_DIR/mchose-battery"
    else
        BIN="$PROJECT_ROOT/target/release/mchose-battery"
    fi
fi

if [[ ! -f "$BIN" ]]; then
    echo "找不到可执行文件：$BIN" >&2
    echo "请先运行：cargo build --release" >&2
    exit 1
fi

if [[ $EUID -ne 0 ]]; then
    exec sudo -- bash "$0" "$BIN"
fi

install -Dm755 "$BIN" /usr/local/bin/mchose-battery
install -Dm644 "$SCRIPT_DIR/90-mchose-battery.rules" \
    /etc/udev/rules.d/90-mchose-battery.rules
install -Dm644 "$SCRIPT_DIR/mchose-battery.desktop" \
    /usr/share/applications/mchose-battery.desktop

udevadm control --reload-rules
udevadm trigger --subsystem-match=hidraw
udevadm settle

echo "已安装：/usr/local/bin/mchose-battery"
echo "udev 规则：/etc/udev/rules.d/90-mchose-battery.rules"
echo
echo "请重新插拔鼠标的 2.4G 接收器 / 数据线（或重新登录），然后运行："
echo "  mchose-battery"
echo
echo "首次排查可运行："
echo "  mchose-battery --scan"
