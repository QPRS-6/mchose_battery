#!/usr/bin/env bash
# Remove the binary, udev rule and per-user autostart entry.
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

if [[ $EUID -ne 0 ]]; then
    exec sudo -- bash "$0" "$@"
fi

rm -f /usr/local/bin/mchose-battery
rm -f /etc/udev/rules.d/90-mchose-battery.rules
rm -f /usr/share/applications/mchose-battery.desktop
udevadm control --reload-rules
udevadm trigger --subsystem-match=hidraw

# The autostart file belongs to the user who invoked sudo.
if [[ -n ${SUDO_USER:-} ]]; then
    USER_HOME=$(getent passwd "$SUDO_USER" | cut -d: -f6)
    rm -f "$USER_HOME/.config/autostart/mchose-battery.desktop"
fi

echo "已卸载 mchose-battery。"
