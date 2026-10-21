#!/bin/sh
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-19
#
# Builds openxplay-relay_<version>_arm64.deb from the Linux release package (the unpacked
# openxplay-relay-<version>-linux-aarch64 directory or its .tar.gz), plus a manifest for the
# flasher. Same files as rust/packaging/install.sh installs, with the unit in /lib/systemd/system
# and /usr/sbin/openxplay-relay-update. Never contains an accessory identity.
#
# Usage: build-deb.sh [--version VERSION] PACKAGE_DIR_OR_TARBALL OUTPUT_DIR
# Needs dpkg-deb; without it the build runs in a debian:bullseye container (Docker). Output is
# byte-identical for identical inputs and SOURCE_DATE_EPOCH (default: newest file in the package).
set -eu
here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
fail() { echo "Error: $*" >&2; exit 1; }
version=
while [ "$#" -gt 2 ]; do
    case "$1" in
    --version) version=$2; shift 2 ;;
    *) fail "Unknown option: $1" ;;
    esac
done
[ "$#" -eq 2 ] || fail "Usage: $0 [--version VERSION] PACKAGE_DIR_OR_TARBALL OUTPUT_DIR"
src=$1
mkdir -p "$2"
out=$(CDPATH= cd -- "$2" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' 0
trap 'exit 1' HUP INT TERM
if [ -f "$src" ]; then
    tar -xzf "$src" -C "$work"
    set -- "$work"/openxplay-relay-*-linux-aarch64
    [ "$#" -eq 1 ] && [ -d "$1" ] || fail "Unexpected tarball layout: $src"
    pkg=$1
else
    pkg=$(CDPATH= cd -- "$src" && pwd)
fi
if [ -z "$version" ]; then
    version=$(basename "$pkg" | sed -n 's/^openxplay-relay-\(.*\)-linux-aarch64$/\1/p')
fi
case "$version" in ''|*[!0-9A-Za-z.+-]*|[!0-9]*) fail "Invalid release version: '$version'" ;; esac
# A release pre-release (0.2.0-rc.1) sorts before its release in Debian as 0.2.0~rc.1.
debversion=$(printf '%s' "$version" | sed 's/-/~/')
for f in bin/openxplay-relay bin/oxp-relay web/receiver/index.html admin/index.html LICENSE LICENSE-GPL-3.0 NOTICE \
    rust/packaging/openxplay-relay.service; do
    [ -f "$pkg/$f" ] || fail "Missing $f in $pkg"
done
if find "$pkg" \( -name identity.pk8 -o -name certificate.p7b -o -name '*.p12' -o -name '*.pfx' -o -name 'offline-mfi*' \) |
    grep -q .; then
    fail 'Refusing a package that contains identity material'
fi
if [ -z "${SOURCE_DATE_EPOCH:-}" ]; then
    if stat -c %Y "$pkg" >/dev/null 2>&1; then mtime='stat -c %Y'; else mtime='stat -f %m'; fi
    # shellcheck disable=SC2086
    SOURCE_DATE_EPOCH=$(find "$pkg" -type f -exec $mtime {} + | sort -n | tail -n 1)
fi
case "$SOURCE_DATE_EPOCH" in ''|*[!0-9]*) fail 'SOURCE_DATE_EPOCH must be an integer' ;; esac
export SOURCE_DATE_EPOCH

if ! command -v dpkg-deb >/dev/null 2>&1; then
    command -v docker >/dev/null 2>&1 || fail 'Needs dpkg-deb or Docker'
    # Docker on macOS may not share these directories with its VM, so the inputs go in as a tar stream.
    mkdir -p "$work/in/packaging"
    cp -R "$pkg" "$work/in/openxplay-relay-$version-linux-aarch64"
    cp -Rf "$here/host" "$here/deb" "$here/build-deb.sh" "$here/openxplay-relay-update.sh" "$here/install.sh" "$here/openxplay-relay.service" "$work/in/packaging/"
    COPYFILE_DISABLE=1 tar --no-xattrs -cf - -C "$work/in" . |
        docker run -i --rm --platform linux/arm64 -e SOURCE_DATE_EPOCH debian:bullseye sh -c \
            "mkdir -p /work /out && tar -xf - -C /work &&
             sh /work/packaging/build-deb.sh --version '$version' /work/openxplay-relay-$version-linux-aarch64 /out >&2 &&
             tar -cf - -C /out ." |
        tar -xf - -C "$out"
    exit 0
fi

sum() { sha256sum "$1" | cut -d' ' -f1; }
root=$work/root
umask 022
install -d "$root/opt/openxplay/bin" "$root/opt/openxplay/share" "$root/lib/systemd/system" "$root/usr/sbin" \
    "$root/opt/openxplay/share/licenses" "$root/usr/share/doc/openxplay-relay"
install -m 0755 "$pkg/bin/openxplay-relay" "$pkg/bin/oxp-relay" "$root/opt/openxplay/bin/"
cp -R "$pkg/web" "$root/opt/openxplay/share/web"
cp -R "$pkg/admin" "$root/opt/openxplay/share/admin"
install -m 0644 "$pkg/LICENSE" "$pkg/LICENSE-GPL-3.0" "$pkg/NOTICE" "$root/opt/openxplay/share/licenses/"
if [ -d "$pkg/licenses" ]; then cp -R "$pkg/licenses/." "$root/opt/openxplay/share/licenses/"; fi
install -m 0644 "$here/openxplay-relay.service" "$root/lib/systemd/system/openxplay-relay.service"
install -m 0755 "$here/openxplay-relay-update.sh" "$root/usr/sbin/openxplay-relay-update"
find "$root/opt/openxplay/share" -type d -exec chmod 0755 {} +
find "$root/opt/openxplay/share" -type f -exec chmod 0644 {} +
cat > "$root/usr/share/doc/openxplay-relay/copyright" <<'EOF'
Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/
Upstream-Name: OpenXPlay Relay
Source: https://github.com/OpenXPlay/OpenXPlay-Relay

Files: *
Copyright: 2026 OpenXPlay.org
License: AGPL-3.0-only AND GPL-3.0-only
 OpenXPlay-written code is AGPL-3.0-only (exemptions: support@openxplay.org); files derived from
 DiPlay or xcertplay are GPL-3.0-only. License texts and component notices (OpenXPlay-Stream,
 third-party crates and web dependencies): /opt/openxplay/share/licenses/.
EOF
sh "$here/install.sh" --install-host-tuning "$root"
ctl=$root/DEBIAN
install -d "$ctl"
for s in preinst postinst prerm postrm; do install -m 0755 "$here/deb/$s" "$ctl/$s"; done
(cd "$root" && find etc -type f | LC_ALL=C sort | sed 's#^#/#') > "$ctl/conffiles"
size=$(du -sk --exclude=DEBIAN "$root" | cut -f1)
cat > "$ctl/control" <<EOF
Package: openxplay-relay
Version: $debversion
Architecture: arm64
Maintainer: OpenXPlay.org <support@openxplay.org>
Installed-Size: $size
Depends: bluez, network-manager, iw, dnsmasq-base, init-system-helpers (>= 1.54)
Section: net
Priority: optional
Homepage: https://openxplay.org
Description: OpenXPlay Relay for Linux (wireless CarPlay to head-unit browsers)
 Authenticates with the iPhone over Bluetooth and Wi-Fi and forwards CarPlay
 to head units through their browser or Carlink apps. Static aarch64 binaries,
 the head-unit web page and the admin page (port 6975). The accessory (MFi)
 identity is never included; supply your own under /var/lib/openxplay/mfi.
EOF
(cd "$root" && find . -type f ! -path './DEBIAN/*' | LC_ALL=C sort | sed 's#^\./##' |
    while IFS= read -r f; do md5sum "$f"; done) > "$ctl/md5sums"
chmod 0644 "$ctl/control" "$ctl/conffiles" "$ctl/md5sums"
find "$root" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
name=openxplay-relay_${debversion}_arm64
dpkg-deb --root-owner-group -Zxz -b "$root" "$out/$name.deb" >/dev/null
digest=$(sum "$out/$name.deb")
cat > "$out/$name.manifest.json" <<EOF
{
  "schema": "org.openxplay.flash-stage/1",
  "stage": "relay",
  "name": "openxplay-relay",
  "version": "$debversion",
  "releaseVersion": "$version",
  "writes": "files",
  "file": "$name.deb",
  "sha256": "$digest",
  "requires": {"dpkgArchitecture": "arm64", "packages": ["bluez", "network-manager", "iw", "dnsmasq-base", "init-system-helpers"]},
  "deviceDir": "/var/cache/openxplay/relay",
  "firstInstall": "dpkg -i /var/cache/openxplay/relay/$name.deb",
  "install": "openxplay-relay-update --sha256 $digest /var/cache/openxplay/relay/$name.deb",
  "success": {"install": "OXP-STAGE relay ok", "rolledBack": "OXP-STAGE relay rolled-back"},
  "verify": "systemctl is-active openxplay-relay.service",
  "rollback": "openxplay-relay-update --rollback",
  "reboot": "none"
}
EOF
echo "Built $out/$name.deb ($digest)"
