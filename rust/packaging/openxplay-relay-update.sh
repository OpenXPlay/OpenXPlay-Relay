#!/bin/sh
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-19
#
# Installs an openxplay-relay .deb, checks that the relay comes up, and reinstalls the previous
# version when it does not. Installed as /usr/sbin/openxplay-relay-update by the package.
#
# Usage: openxplay-relay-update [--keep N] [--sha256 HEX] PACKAGE.deb
#        openxplay-relay-update --rollback     reinstall the newest cached version below the current
#        openxplay-relay-update --list         cached versions
# Over adb: push the .deb, then run this. From the relay itself (admin page), run it outside the
# relay's own cgroup, because the update restarts the relay:
#   systemd-run --unit=openxplay-relay-update --collect /usr/sbin/openxplay-relay-update FILE.deb
# Last line for tools: "OXP-STAGE relay ok|rolled-back|failed ...". Exit 0 ok, 2 rolled back, 1 error.
set -eu
cache=${OPENXPLAY_RELAY_CACHE:-/var/cache/openxplay/relay}
unit=openxplay-relay.service
wait_active=${OPENXPLAY_RELAY_UPDATE_TIMEOUT:-60}
settle=${OPENXPLAY_RELAY_UPDATE_SETTLE:-15}
keep=3
want_sha=
export LC_ALL=C DEBIAN_FRONTEND=noninteractive
say() { echo "openxplay-relay-update: $*"; }
fail() { echo "OXP-STAGE relay failed: $*" >&2; exit 1; }
sum() { sha256sum "$1" | cut -d' ' -f1; }
installed() { dpkg-query -W -f='${Status} ${Version}' openxplay-relay 2>/dev/null | awk '$3 == "installed" {print $4}'; }
cached() { printf '%s/openxplay-relay_%s_arm64.deb' "$cache" "$1"; }
versions() {
    for f in "$cache"/openxplay-relay_*_arm64.deb; do
        [ -f "$f" ] || continue
        v=${f##*/openxplay-relay_}; echo "${v%_arm64.deb}"
    done
}
# Uptime, not wall-clock time: the device clock can jump when time sync catches up.
uptime_s() { read -r up _ < /proc/uptime; echo "${up%%.*}"; }
prop() { systemctl show -p "$1" --value "$unit"; }
healthy() {
    if ! systemctl is-enabled "$unit" >/dev/null 2>&1; then
        say "$unit is not enabled; skipping the health check"
        return 0
    fi
    deadline=$(($(uptime_s) + wait_active))
    until [ "$(prop ActiveState)" = active ]; do
        [ "$(uptime_s)" -lt "$deadline" ] || return 1
        sleep 1
    done
    pid=$(prop MainPID); restarts=$(prop NRestarts)
    sleep "$settle"
    [ "$(prop ActiveState)" = active ] && [ "$(prop MainPID)" = "$pid" ] && [ "$(prop NRestarts)" = "$restarts" ]
}
install_deb() { dpkg --force-confdef --force-confold -i "$1"; }
# Highest cached version strictly below $1 (or below everything if $1 is empty).
previous_of() {
    best=
    for v in $(versions); do
        if [ -n "$1" ] && ! dpkg --compare-versions "$v" lt "$1"; then continue; fi
        if [ -z "$best" ] || dpkg --compare-versions "$v" gt "$best"; then best=$v; fi
    done
    echo "$best"
}
prune() {
    current=$(installed); prev=$(previous_of "$current")
    n=0
    for v in $(versions | sort -r -V); do
        n=$((n + 1))
        if [ "$v" = "$current" ] || [ "$v" = "$prev" ] || [ "$n" -le "$keep" ]; then continue; fi
        rm -f "$(cached "$v")"; say "removed cached $v"
    done
}

mode=install
while [ "$#" -gt 0 ]; do
    case "$1" in
    --keep) keep=$2; shift 2 ;;
    --sha256) want_sha=$2; shift 2 ;;
    --rollback) mode=rollback; shift ;;
    --list) mode=list; shift ;;
    -*) fail "unknown option $1" ;;
    *) break ;;
    esac
done
[ "$(id -u)" -eq 0 ] || fail 'run as root'
case "$keep" in ''|*[!0-9]*) fail '--keep needs a number' ;; esac
mkdir -p "$cache"; chmod 0700 "$cache"

if [ "$mode" = list ]; then
    current=$(installed)
    for v in $(versions); do if [ "$v" = "$current" ]; then echo "$v (installed)"; else echo "$v"; fi; done
    exit 0
fi

old=$(installed)
if [ "$mode" = rollback ]; then
    [ -n "$old" ] || fail 'openxplay-relay is not installed as a package'
    target=$(previous_of "$old")
    [ -n "$target" ] || fail "no cached version below $old"
    say "rolling back $old -> $target"
    install_deb "$(cached "$target")" || fail "dpkg failed for $target"
    healthy || fail "relay not healthy after rolling back to $target"
    echo "OXP-STAGE relay ok version=$target rolled-back-from=$old"
    exit 0
fi

[ "$#" -eq 1 ] || fail 'usage: openxplay-relay-update [--keep N] [--sha256 HEX] PACKAGE.deb'
deb=$1
[ -f "$deb" ] || fail "no such file: $deb"
if [ -n "$want_sha" ]; then [ "$(sum "$deb")" = "$want_sha" ] || fail 'SHA-256 mismatch'; fi
[ "$(dpkg-deb -f "$deb" Package)" = openxplay-relay ] || fail 'not an openxplay-relay package'
[ "$(dpkg-deb -f "$deb" Architecture)" = arm64 ] || fail 'not an arm64 package'
new=$(dpkg-deb -f "$deb" Version)
case "$new" in ''|*[!0-9A-Za-z.+~-]*) fail "unexpected version: $new" ;; esac
target=$(cached "$new")
if [ "$(cd "$(dirname "$deb")" && pwd)/$(basename "$deb")" != "$target" ]; then
    cp "$deb" "$target.tmp" && mv -f "$target.tmp" "$target"
fi
fallback=
if [ -n "$old" ] && [ "$old" != "$new" ] && [ -f "$(cached "$old")" ]; then fallback=$(cached "$old"); fi
[ -n "$fallback" ] || say "no cached package of the current version ${old:-none}: no automatic rollback"
say "installing $new (current: ${old:-none})"
if install_deb "$target" && healthy; then
    prune
    echo "OXP-STAGE relay ok version=$new previous=${old:-none}"
    exit 0
fi
[ -n "$fallback" ] || fail "relay $new did not come up and there is nothing to roll back to"
say "relay $new did not come up; reinstalling $old"
install_deb "$fallback" || fail "rollback to $old failed"
healthy || fail "relay not healthy after rolling back to $old"
# A later --rollback must never pick the build that just failed.
rm -f "$target"
echo "OXP-STAGE relay rolled-back version=$old failed=$new"
exit 2
