#!/bin/sh
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-15
# Usage: [DESTDIR=/absolute/staging/root] install.sh OPENXPLAY_RELAY_BINARY OXP_RELAY_BINARY WEB_ASSETS ADMIN_ASSETS [PLUGINS_BUNDLE]
set -eu
# The Debian builder uses this same host payload installer.
install_host_tuning() {
  host_dir="$(dirname "$0")/host"
  install -d -m 0755 "$1/etc/NetworkManager/conf.d" "$1/usr/lib/openxplay-relay"
  install -m 0644 "$host_dir/openxplay-relay-wifi-powersave.conf" \
    "$host_dir/openxplay-relay-wifi-scan-mac.conf" "$1/etc/NetworkManager/conf.d/"
  install -m 0755 "$host_dir/clock-guard" "$1/usr/lib/openxplay-relay/clock-guard"
}
if [ "${1:-}" = --install-host-tuning ]; then
  [ "$#" -eq 2 ] && [ -n "$2" ] || exit 2
  case "$2" in /*) ;; *) exit 2 ;; esac
  install_host_tuning "${2%/}"
  exit 0
fi
if [ "$#" -ne 4 ] && [ "$#" -ne 5 ]; then
  echo "Usage: $0 OPENXPLAY_RELAY_BINARY OXP_RELAY_BINARY WEB_ASSETS ADMIN_ASSETS [PLUGINS_BUNDLE]" >&2
  exit 2
fi
DESTDIR=${DESTDIR:-}
case "$DESTDIR" in ""|/*) ;; *) echo "DESTDIR must be absolute" >&2; exit 2 ;; esac
case "$DESTDIR" in ''|*[!/]*) ;; *) echo "Use an empty DESTDIR for live installation" >&2; exit 2 ;; esac
DESTDIR=${DESTDIR%/}
if [ -z "$DESTDIR" ] && [ "$(id -u)" -ne 0 ]; then
  echo "Run as root on the target device." >&2
  exit 1
fi
for binary in "$1" "$2"; do test -f "$binary"; done
# The receiver page lives at receiver/index.html; the admin page at the root.
test -f "$3/receiver/index.html"
test -f "$4/index.html"
# Identity material is supplied separately by the operator, never by this package.
for assets in "$3" "$4"; do
  if find "$assets" -type f \( -name identity.pk8 -o -name certificate.p7b \) | read -r ignored; then
    echo "Refusing asset directory containing MFi identity material: $assets" >&2
    exit 1
  fi
done
# Optional bundle is a prebuilt host/ + p/ install directory; no runtime dependency.
plugins_stage=
if [ "$#" -eq 5 ]; then
  plugins_stage=$(mktemp -d)
  trap 'rm -rf "$plugins_stage"' 0
  python3 "$(dirname "$0")/plugins-bundle.py" "$5" "$plugins_stage"
fi
install -d -m 0755 "$DESTDIR/opt/openxplay/bin" "$DESTDIR/opt/openxplay/share/web" "$DESTDIR/opt/openxplay/share/admin"
install -m 0755 "$1" "$DESTDIR/opt/openxplay/bin/openxplay-relay"
install -m 0755 "$2" "$DESTDIR/opt/openxplay/bin/oxp-relay"
cp -Rf "$3/." "$DESTDIR/opt/openxplay/share/web/"
cp -Rf "$4/." "$DESTDIR/opt/openxplay/share/admin/"
if [ -n "$plugins_stage" ]; then
  install -d -m 0755 "$DESTDIR/opt/openxplay/share/plugins"
  cp -Rf "$plugins_stage/." "$DESTDIR/opt/openxplay/share/plugins/"
  find "$DESTDIR/opt/openxplay/share/plugins" -type d -exec chmod 0755 {} +
  find "$DESTDIR/opt/openxplay/share/plugins" -type f -exec chmod 0644 {} +
fi
# Preserve both project licenses and component notices with the installed build.
# A device bundle ships them next to this script; the repository keeps them at its root.
license_dir="$(dirname "$0")/licenses"
[ -f "$license_dir/LICENSE" ] || license_dir="$(dirname "$0")/../.."
install -d -m 0755 "$DESTDIR/opt/openxplay/share/licenses"
install -m 0644 "$license_dir/LICENSE" "$license_dir/LICENSE-GPL-3.0" "$license_dir/NOTICE" "$DESTDIR/opt/openxplay/share/licenses/"
install -d -m 0700 "$DESTDIR/var/lib/openxplay" "$DESTDIR/var/lib/openxplay/mfi"
# Older device bundles ship only install.sh and the unit.
if [ -d "$(dirname "$0")/host" ]; then
  install_host_tuning "$DESTDIR"
else
  echo "Host tuning files not bundled next to install.sh; skipped." >&2
fi
install -d -m 0755 "$DESTDIR/etc/systemd/system"
install -m 0644 "$(dirname "$0")/openxplay-relay.service" "$DESTDIR/etc/systemd/system/openxplay-relay.service"
if [ -n "$DESTDIR" ]; then
  systemctl --root="$DESTDIR" enable openxplay-relay.service
else
  nmcli general reload >/dev/null 2>&1 || :
  systemctl daemon-reload
  systemctl enable openxplay-relay.service
fi
printf '%s\n' 'Installed. Supply your MFi identity separately:' \
  '  /var/lib/openxplay/mfi/identity.pk8' \
  '  /var/lib/openxplay/mfi/certificate.p7b' \
  'Set both files to mode 0600, then run systemctl start openxplay-relay.service.' \
  'First setup: https://openxplay-linux.local:6975 (password: openxplay).' \
  'Requires NetworkManager (nmcli), iw, dnsmasq-base (shared-mode DHCP),' \
  'BlueZ (bluetoothctl, btmgmt, sdptool, hciconfig), and kernel RFCOMM support.'
