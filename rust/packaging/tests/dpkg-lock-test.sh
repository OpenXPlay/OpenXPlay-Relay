#!/bin/sh
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-21
#
# Real dpkg lock checks in a disposable Debian arm64 root, with no network or extra packages.
# Usage: dpkg-lock-test.sh [PATH_TO_OPENXPLAY_RELAY_UPDATE]
set -eu
update=${1:-$(dirname "$0")/../openxplay-relay-update.sh}
[ "$(id -u)" -eq 0 ] && [ "$(dpkg --print-architecture)" = arm64 ] || {
    echo 'Run as root inside a disposable Debian arm64 root' >&2; exit 2;
}
for package in openxplay-relay openxplay-test-dpkg-lock; do
    [ -z "$(dpkg-query -W -f='${Version}' "$package" 2>/dev/null || :)" ] || {
        echo "Disposable root must not already contain $package" >&2; exit 2;
    }
done
work=$(mktemp -d)
holder=; updater=; pass=0
real_dpkg=$(command -v dpkg)
real_lslocks=$(command -v lslocks)
unset DPKG_FRONTEND_LOCKED
cleanup() {
    touch "$work/unlock"
    if [ -n "$holder" ]; then wait "$holder" || :; fi
    if [ -n "$updater" ]; then kill "$updater" 2>/dev/null || :; wait "$updater" || :; fi
    "$real_dpkg" --purge openxplay-relay openxplay-test-dpkg-lock >/dev/null 2>&1 || :
    rm -rf "$work"
}
trap cleanup 0
trap 'exit 1' HUP INT TERM
check() {
    label=$1; shift
    if "$@"; then echo "PASS $label"; pass=$((pass + 1)); else echo "FAIL $label" >&2; exit 1; fi
}
mkdir -p "$work/relay/DEBIAN" "$work/holder/DEBIAN" "$work/stub"
for version in 9.8.1 9.8.2; do
    printf 'Package: openxplay-relay\nVersion: %s\nArchitecture: arm64\nMaintainer: test <test@example.test>\nDescription: updater lock fixture\n' "$version" > "$work/relay/DEBIAN/control"
    dpkg-deb -b "$work/relay" "$work/$version.deb" >/dev/null
done
printf 'Package: openxplay-test-dpkg-lock\nVersion: 1\nArchitecture: all\nMaintainer: test <test@example.test>\nDescription: dpkg lock holder\n' > "$work/holder/DEBIAN/control"
cat > "$work/holder/DEBIAN/postinst" <<EOF
#!/bin/sh
touch '$work/locked'
while [ ! -e '$work/unlock' ]; do sleep 0.1; done
EOF
chmod 0755 "$work/holder/DEBIAN/postinst"
dpkg-deb -b "$work/holder" "$work/holder.deb" >/dev/null
cat > "$work/stub/dpkg" <<EOF
#!/bin/sh
for arg do
    if [ "\$arg" = -i ]; then echo install >> '$work/dpkg-i'; fi
done
exec '$real_dpkg' "\$@"
EOF
cat > "$work/stub/lslocks" <<EOF
#!/bin/sh
if [ -e '$work/hide-lock' ]; then rm -f '$work/hide-lock'; exit 0; fi
exec '$real_lslocks' "\$@"
EOF
cat > "$work/stub/systemctl" <<'EOF'
#!/bin/sh
case "$1" in
is-enabled) exit 0 ;;
show)
    case "$3" in ActiveState) echo active ;; MainPID) echo 4242 ;; NRestarts) echo 0 ;; esac ;;
esac
EOF
chmod 0755 "$work/stub/dpkg" "$work/stub/lslocks" "$work/stub/systemctl"
export PATH="$work/stub:$PATH" OPENXPLAY_RELAY_CACHE="$work/cache" \
    OPENXPLAY_RELAY_UPDATE_TIMEOUT=0 OPENXPLAY_RELAY_UPDATE_SETTLE=0 OPENXPLAY_RELAY_UPDATE_LOCK_TIMEOUT=10
start_holder() {
    rm -f "$work/locked" "$work/unlock" "$work/dpkg-i"
    if [ "${1:-}" = backend ]; then
        DPKG_FRONTEND_LOCKED=1 "$real_dpkg" -i "$work/holder.deb" > "$work/holder.out" 2>&1 &
    else
        "$real_dpkg" -i "$work/holder.deb" > "$work/holder.out" 2>&1 &
    fi
    holder=$!
    n=0
    until [ -e "$work/locked" ]; do
        n=$((n + 1))
        if [ "$n" -gt 100 ] || ! kill -0 "$holder" 2>/dev/null; then cat "$work/holder.out" >&2; exit 1; fi
        sleep 0.1
    done
}
release_holder() { touch "$work/unlock"; wait "$holder"; holder=; }
start_update() {
    sh "$update" "$@" > "$work/out" 2> "$work/err" &
    updater=$!
    n=0
    until grep -q '^OXP-STAGE relay waiting: dpkg is busy (' "$work/out"; do
        n=$((n + 1))
        if [ "$n" -gt 100 ] || ! kill -0 "$updater" 2>/dev/null; then cat "$work/out" "$work/err" >&2; exit 1; fi
        sleep 0.1
    done
}
finish_update() {
    if wait "$updater"; then rc=0; else rc=$?; fi
    updater=
    check 'update succeeds after lock release' test "$rc" -eq 0
    check 'one waiting line' test "$(grep -c '^OXP-STAGE relay waiting:' "$work/out")" -eq 1
    check 'expected number of dpkg install attempts' test "$(wc -l < "$work/dpkg-i")" -eq "${1:-1}"
}

start_holder
lslocks --noheadings --raw --output PATH > "$work/locks"
check 'real frontend lock held' grep -qx /var/lib/dpkg/lock-frontend "$work/locks"
check 'real database lock held' grep -qx /var/lib/dpkg/lock "$work/locks"
start_update "$work/9.8.1.deb"
check 'no dpkg install while locked' test ! -e "$work/dpkg-i"
release_holder
finish_update
check 'first install completes' grep -qx 'OXP-STAGE relay ok version=9.8.1 previous=none' "$work/out"

start_holder backend
lslocks --noheadings --raw --output PATH > "$work/locks"
check 'real database lock held without frontend lock' grep -qx /var/lib/dpkg/lock "$work/locks"
check 'frontend lock is free' test "$(grep -c '^/var/lib/dpkg/lock-frontend$' "$work/locks" || :)" -eq 0
if OPENXPLAY_RELAY_UPDATE_LOCK_TIMEOUT=1 sh "$update" "$work/9.8.2.deb" > "$work/out" 2> "$work/err"; then rc=0; else rc=$?; fi
check 'lock timeout exits 1' test "$rc" -eq 1
check 'lock timeout failure on stderr' grep -qx 'OXP-STAGE relay failed: dpkg is locked by another process' "$work/err"
check 'no failure on stdout' test "$(grep -c '^OXP-STAGE relay failed:' "$work/out" || :)" -eq 0
check 'no install or rollback on timeout' test ! -e "$work/dpkg-i"
check 'installed version unchanged' test "$(dpkg-query -W -f='${Version}' openxplay-relay)" = 9.8.1
release_holder

# Hide one probe to exercise dpkg's real lock error if a lock is taken after the check.
start_holder
touch "$work/hide-lock"
start_update "$work/9.8.2.deb"
check 'dpkg encountered the raced lock once' test "$(wc -l < "$work/dpkg-i")" -eq 1
check 'raced lock did not change installed version' test "$(dpkg-query -W -f='${Version}' openxplay-relay)" = 9.8.1
release_holder
finish_update 2
check 'upgrade succeeds after raced lock' grep -qx 'OXP-STAGE relay ok version=9.8.2 previous=9.8.1' "$work/out"

start_holder
start_update --rollback
check 'no rollback install while locked' test ! -e "$work/dpkg-i"
release_holder
finish_update
check 'manual rollback completes' grep -qx 'OXP-STAGE relay ok version=9.8.1 rolled-back-from=9.8.2' "$work/out"
echo "PASS $pass real dpkg lock checks"
