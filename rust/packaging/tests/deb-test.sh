#!/bin/sh
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-19
#
# Tests openxplay-relay .deb installs and openxplay-relay-update (upgrade, automatic rollback of a
# relay that does not come up, manual rollback, cache pruning) inside a disposable Debian 11/12/13 arm64
# root (container or chroot). systemctl is a stub that reports the relay "failed" for one version.
#
# Usage, as root inside that root: deb-test.sh [--prepare] [--debian bullseye|bookworm|trixie] OPENXPLAY_RELAY.deb
#   --prepare  apt-get install runtime and test dependencies (network needed)
#   --debian   select the debian:SUITE disposable root; defaults to bullseye
set -eu
prepare=false
debian=bullseye
while [ "$#" -gt 1 ]; do
    case "$1" in
    --prepare) prepare=true; shift ;;
    --debian) debian=${2#debian:}; shift 2 ;;
    *) echo "Unknown option: $1" >&2; exit 2 ;;
    esac
done
[ "$#" -eq 1 ] || { echo "Usage: $0 [--prepare] [--debian SUITE] OPENXPLAY_RELAY.deb" >&2; exit 2; }
case "$debian" in bullseye) os=11 ;; bookworm) os=12 ;; trixie) os=13 ;; *) echo 'Unknown Debian suite' >&2; exit 2 ;; esac
[ "$(id -u)" -eq 0 ] || { echo 'Run as root inside a disposable root' >&2; exit 2; }
grep -qx "VERSION_ID=\"$os\"" /etc/os-release || { echo "Run in debian:$debian (Debian $os)" >&2; exit 2; }
# All tests here are generic: no UFI003 kernel, adapter or live Wi-Fi is required.
export DEBIAN_FRONTEND=noninteractive LC_ALL=C
if "$prepare"; then
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends bluez network-manager iw dnsmasq-base python3 systemd init-system-helpers >/dev/null
fi
python3 "$(dirname "$0")/host-tuning-test.py"
python3 "$(dirname "$0")/update-test.py"
work=$(mktemp -d)
cp "$1" "$work/base.deb"
pass=0; failed=0
check() { if sh -c "$2" >/dev/null 2>&1; then echo "PASS $1"; pass=$((pass + 1)); else echo "FAIL $1"; failed=$((failed + 1)); fi; }
installed() { dpkg-query -W -f='${Status} ${Version}' openxplay-relay 2>/dev/null | awk '$3 == "installed" {print $4}'; }
# Same files, another version.
repack() {
    rm -rf "$work/r"; dpkg-deb -R "$work/base.deb" "$work/r"
    sed -i "s/^Version: .*/Version: $1/" "$work/r/DEBIAN/control"
    dpkg-deb --root-owner-group -b "$work/r" "$work/openxplay-relay_$1_arm64.deb" >/dev/null
}
v1=$(dpkg-deb -f "$work/base.deb" Version)
cp "$work/base.deb" "$work/openxplay-relay_${v1}_arm64.deb"
for v in 9.0.1 9.0.2 9.0.3 9.0.4 9.0.5; do repack "$v"; done
# systemctl stub: the relay is "failed" while version $(cat bad) is installed, active otherwise.
stub=$work/stub; mkdir -p "$stub"
cat > "$stub/systemctl" <<EOF
#!/bin/sh
bad=\$(cat '$work/bad' 2>/dev/null || :)
v=\$(dpkg-query -W -f='\${Version}' openxplay-relay 2>/dev/null || :)
case "\$1" in
is-enabled) [ -e /etc/systemd/system/multi-user.target.wants/openxplay-relay.service ] && { echo enabled; exit 0; }; echo disabled; exit 1 ;;
show)
    case "\$3" in
    ActiveState) if [ -n "\$bad" ] && [ "\$v" = "\$bad" ]; then echo failed; else echo active; fi ;;
    MainPID) echo 4242 ;;
    NRestarts) echo 0 ;;
    esac ;;
*) exec /bin/systemctl "\$@" ;;
esac
EOF
chmod 0755 "$stub/systemctl"
export PATH="$stub:$PATH" OPENXPLAY_RELAY_UPDATE_TIMEOUT=2 OPENXPLAY_RELAY_UPDATE_SETTLE=1
mkdir -p /var/lib/openxplay/mfi
printf 'test-only-not-an-identity\n' > /var/lib/openxplay/mfi/identity.pk8
chmod -R go-rwx /var/lib/openxplay
before=$(sha256sum /var/lib/openxplay/mfi/identity.pk8)
dpkg-deb -x "$work/base.deb" "$work/x"
# Verify the Debian builder installed the canonical host sources byte for byte.
for policy in powersave scan-mac; do
    check "canonical $policy policy" "cmp '$(dirname "$0")/../host/openxplay-relay-wifi-$policy.conf' '$work/x/etc/NetworkManager/conf.d/openxplay-relay-wifi-$policy.conf'"
    check "$policy policy mode" "[ \"\$(stat -c %a '$work/x/etc/NetworkManager/conf.d/openxplay-relay-wifi-$policy.conf')\" = 644 ]"
done
check 'canonical clock guard' "cmp '$(dirname "$0")/../host/clock-guard' '$work/x/usr/lib/openxplay-relay/clock-guard'"
check 'clock guard mode' "[ \"\$(stat -c %a '$work/x/usr/lib/openxplay-relay/clock-guard')\" = 755 ]"
update=$work/x/usr/sbin/openxplay-relay-update
sh "$(dirname "$0")/dpkg-lock-test.sh" "$update"
run() { "$@" > "$work/out" 2>&1; rc=$?; tail -n 1 "$work/out"; return $rc; }

# A minimal 0.1.0 device package owns the old paths, including the clock drop-in.
# No UFI003 hardware is needed to exercise dpkg file ownership on any Debian suite.
old=$work/old-device
mkdir -p "$old/DEBIAN" "$old/etc/NetworkManager/conf.d" "$old/usr/lib/openxplay-ufi003" \
    "$old/lib/systemd/system/openxplay-relay.service.d"
printf 'Package: openxplay-ufi003\nVersion: 0.1.0\nArchitecture: arm64\nMaintainer: t <t@t>\nDescription: old device ownership fixture\n' > "$old/DEBIAN/control"
for policy in powersave scan-mac; do
    cp -f "$work/x/etc/NetworkManager/conf.d/openxplay-relay-wifi-$policy.conf" "$old/etc/NetworkManager/conf.d/openxplay-wifi-$policy.conf"
    echo "/etc/NetworkManager/conf.d/openxplay-wifi-$policy.conf" >> "$old/DEBIAN/conffiles"
done
cp -f "$work/x/usr/lib/openxplay-relay/clock-guard" "$old/usr/lib/openxplay-ufi003/clock-guard"
printf '[Service]\nExecStartPre=-/usr/lib/openxplay-ufi003/clock-guard\n' > "$old/lib/systemd/system/openxplay-relay.service.d/ufi003-clock.conf"
dpkg-deb --root-owner-group -b "$old" "$work/old-device.deb" >/dev/null
dpkg -i "$work/old-device.deb" >/dev/null
old_digest=$(find /etc/NetworkManager/conf.d -name 'openxplay-wifi-*.conf' | sort | xargs sha256sum)
echo '== First install through the updater (no package yet)'
check 'first install ok' "$update $work/openxplay-relay_${v1}_arm64.deb | tail -n 1 | grep -q '^OXP-STAGE relay ok version=$v1 previous=none'"
check 'enabled' "[ -L /etc/systemd/system/multi-user.target.wants/openxplay-relay.service ]"
check 'cached' "[ -f /var/cache/openxplay/relay/openxplay-relay_${v1}_arm64.deb ]"
check 'cache mode 0700' "[ \"\$(stat -c %a /var/cache/openxplay/relay)\" = 700 ]"
for c in nmcli iw dnsmasq bluetoothctl btmgmt sdptool hciconfig; do
    check "dependency command: $c" "command -v $c"
done
# Inspect package file lists as well as PATH: deprecated BlueZ utilities must
# still be shipped by bluez on each supported Debian suite.
for c in bluetoothctl btmgmt sdptool hciconfig; do
    check "bluez provides $c" "dpkg-query -L bluez | grep -Eq '/(s?bin)/$c$'"
done
check 'dnsmasq-base declared' "dpkg-query -W -f='\${Depends}' openxplay-relay | grep -q dnsmasq-base"
check 'generic policies are conffiles' "dpkg-query -W -f='\${Conffiles}' openxplay-relay | grep -q /etc/NetworkManager/conf.d/openxplay-relay-wifi-powersave.conf && dpkg-query -W -f='\${Conffiles}' openxplay-relay | grep -q /etc/NetworkManager/conf.d/openxplay-relay-wifi-scan-mac.conf"
check 'power save disabled' "grep -qx 'wifi.powersave = 2' /etc/NetworkManager/conf.d/openxplay-relay-wifi-powersave.conf"
check 'scan policy covers all Wi-Fi devices' "grep -qx 'match-device=type:wifi' /etc/NetworkManager/conf.d/openxplay-relay-wifi-scan-mac.conf && grep -qx 'wifi.scan-rand-mac-address=no' /etc/NetworkManager/conf.d/openxplay-relay-wifi-scan-mac.conf"
check 'clock guard in main unit' "grep -qx 'ExecStartPre=-/usr/lib/openxplay-relay/clock-guard' /lib/systemd/system/openxplay-relay.service && [ -x /usr/lib/openxplay-relay/clock-guard ]"
check 'old device policies unchanged' "[ \"\$(find /etc/NetworkManager/conf.d -name 'openxplay-wifi-*.conf' | sort | xargs sha256sum)\" = '$old_digest' ]"
check 'old device guard still installed' "[ -x /usr/lib/openxplay-ufi003/clock-guard ] && [ -f /lib/systemd/system/openxplay-relay.service.d/ufi003-clock.conf ]"
# Read the builder output when supplied alongside the .deb; Depends and manifest must agree.
manifest=${1%.deb}.manifest.json
if [ -f "$manifest" ]; then
    MANIFEST="$manifest" python3 - <<'MANIFEST_TEST'
import json, os, subprocess
with open(os.environ["MANIFEST"]) as f:
    m = json.load(f)
assert "transport" not in m and "debianVersion" not in m["requires"]
assert m["requires"]["dpkgArchitecture"] == "arm64"
deps = subprocess.check_output(["dpkg-query", "-W", "-f=${Depends}", "openxplay-relay"], text=True)
assert set(m["requires"]["packages"]) == {d.strip().split()[0] for d in deps.split(",")}
MANIFEST_TEST
fi
echo '== Upgrade'
check 'upgrade ok' "openxplay-relay-update $work/openxplay-relay_9.0.1_arm64.deb | tail -n 1 | grep -q '^OXP-STAGE relay ok version=9.0.1 previous=$v1'"
check 'installed 9.0.1' "[ \"\$(dpkg-query -W -f='\${Version}' openxplay-relay)\" = 9.0.1 ]"
echo '== A relay that does not come up is rolled back'
echo 9.0.2 > "$work/bad"
set +e; run openxplay-relay-update "$work/openxplay-relay_9.0.2_arm64.deb"; rc=$?; set -e
check 'exit code 2' "[ $rc -eq 2 ]"
check 'rolled-back line' "tail -n 1 $work/out | grep -q '^OXP-STAGE relay rolled-back version=9.0.1 failed=9.0.2'"
check 'back on 9.0.1' "[ \"\$(dpkg-query -W -f='\${Version}' openxplay-relay)\" = 9.0.1 ]"
check 'failed build dropped from the cache' "[ ! -e /var/cache/openxplay/relay/openxplay-relay_9.0.2_arm64.deb ]"
rm -f "$work/bad"
echo '== Manual rollback'
check 'rollback to the version below' "openxplay-relay-update --rollback | tail -n 1 | grep -q '^OXP-STAGE relay ok version=$v1 rolled-back-from=9.0.1'"
check 'installed v1' "[ \"\$(dpkg-query -W -f='\${Version}' openxplay-relay)\" = '$v1' ]"
echo '== Refusals'
check 'checksum mismatch refused' "! openxplay-relay-update --sha256 0000000000000000000000000000000000000000000000000000000000000000 $work/openxplay-relay_9.0.3_arm64.deb"
check 'still v1 after refusal' "[ \"\$(dpkg-query -W -f='\${Version}' openxplay-relay)\" = '$v1' ]"
rm -rf "$work/other"; mkdir -p "$work/other/DEBIAN"
printf 'Package: something-else\nVersion: 1\nArchitecture: arm64\nMaintainer: t <t@t>\nDescription: t\n' > "$work/other/DEBIAN/control"
dpkg-deb -b "$work/other" "$work/other.deb" >/dev/null
check 'other package refused' "! openxplay-relay-update $work/other.deb"
echo '== Pruning keeps the newest, the installed and the previous version'
for v in 9.0.3 9.0.4 9.0.5; do openxplay-relay-update --keep 2 "$work/openxplay-relay_${v}_arm64.deb" >/dev/null; done
check 'installed 9.0.5' "[ \"\$(dpkg-query -W -f='\${Version}' openxplay-relay)\" = 9.0.5 ]"
check 'cache pruned to 9.0.4 and 9.0.5' "[ \"\$(openxplay-relay-update --list | tr '\n' ' ')\" = '9.0.4 9.0.5 (installed) ' ]"
check 'identity untouched' "[ \"\$(sha256sum /var/lib/openxplay/mfi/identity.pk8)\" = '$before' ]"
echo '== Remove and purge'
dpkg --purge openxplay-relay >/dev/null
check 'purged' "[ -z \"\$(installed)\" ] && [ ! -e /usr/sbin/openxplay-relay-update ]"
check 'cache removed on purge' "[ ! -e /var/cache/openxplay/relay ]"
check 'identity kept on purge' "[ \"\$(sha256sum /var/lib/openxplay/mfi/identity.pk8)\" = '$before' ]"
check 'generic policies gone on purge' "[ ! -e /etc/NetworkManager/conf.d/openxplay-relay-wifi-powersave.conf ] && [ ! -e /etc/NetworkManager/conf.d/openxplay-relay-wifi-scan-mac.conf ]"
check 'old device survives relay purge' "[ -f /etc/NetworkManager/conf.d/openxplay-wifi-powersave.conf ] && [ -x /usr/lib/openxplay-ufi003/clock-guard ]"
dpkg --purge openxplay-ufi003 >/dev/null
echo; echo "Result: $pass passed, $failed failed"
[ "$failed" -eq 0 ]
