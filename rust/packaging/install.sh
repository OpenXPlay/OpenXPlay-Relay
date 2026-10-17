#!/bin/sh
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-15
# Usage: [DESTDIR=/absolute/staging/root] install.sh OPENXPLAY_RELAY_BINARY OXP_RELAY_BINARY WEB_ASSETS ADMIN_ASSETS
set -eu
if [ "$#" -ne 4 ]; then
  echo "Usage: $0 OPENXPLAY_RELAY_BINARY OXP_RELAY_BINARY WEB_ASSETS ADMIN_ASSETS" >&2
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
install -d -m 0755 "$DESTDIR/opt/openxplay/bin" "$DESTDIR/opt/openxplay/share/web" "$DESTDIR/opt/openxplay/share/admin"
install -m 0755 "$1" "$DESTDIR/opt/openxplay/bin/openxplay-relay"
install -m 0755 "$2" "$DESTDIR/opt/openxplay/bin/oxp-relay"
cp -Rf "$3/." "$DESTDIR/opt/openxplay/share/web/"
cp -Rf "$4/." "$DESTDIR/opt/openxplay/share/admin/"
# Preserve both project licenses and component notices with the installed build.
# A device bundle ships them next to this script; the repository keeps them at its root.
license_dir="$(dirname "$0")/licenses"
[ -f "$license_dir/LICENSE" ] || license_dir="$(dirname "$0")/../.."
install -d -m 0755 "$DESTDIR/opt/openxplay/share/licenses"
install -m 0644 "$license_dir/LICENSE" "$license_dir/LICENSE-GPL-3.0" "$license_dir/NOTICE" "$DESTDIR/opt/openxplay/share/licenses/"
install -d -m 0700 "$DESTDIR/var/lib/openxplay" "$DESTDIR/var/lib/openxplay/mfi"
install -m 0644 "$(dirname "$0")/openxplay-relay.service" "$DESTDIR/etc/systemd/system/openxplay-relay.service"
if [ -n "$DESTDIR" ]; then
  systemctl --root="$DESTDIR" enable openxplay-relay.service
else
  systemctl daemon-reload
  systemctl enable openxplay-relay.service
fi
printf '%s\n' 'Installed. Supply your MFi identity separately:' \
  '  /var/lib/openxplay/mfi/identity.pk8' \
  '  /var/lib/openxplay/mfi/certificate.p7b' \
  'Set both files to mode 0600, then run systemctl start openxplay-relay.service.' \
  'First setup: https://openxplay-linux.local:6975 (password: openxplay).' \
  'Requires BlueZ (sdptool, hciconfig), NetworkManager (nmcli), and iw.'
