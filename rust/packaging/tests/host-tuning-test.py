#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-20
"""Unprivileged staging and clock-guard checks; Linux also checks the real timeout."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

packaging = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp) / "root"
    subprocess.run(["sh", str(packaging / "install.sh"), "--install-host-tuning", str(root)], check=True)
    for name, relative, mode in (
        ("openxplay-relay-wifi-powersave.conf", "etc/NetworkManager/conf.d", 0o644),
        ("openxplay-relay-wifi-scan-mac.conf", "etc/NetworkManager/conf.d", 0o644),
        ("clock-guard", "usr/lib/openxplay-relay", 0o755),
    ):
        source = packaging / "host" / name
        installed = root / relative / name
        assert installed.read_bytes() == source.read_bytes()
        assert installed.stat().st_mode & 0o777 == mode
        assert source.stat().st_mode & 0o777 == mode
    policies = root / "etc/NetworkManager/conf.d"
    assert "wifi.powersave = 2" in (policies / "openxplay-relay-wifi-powersave.conf").read_text()
    scan = (policies / "openxplay-relay-wifi-scan-mac.conf").read_text()
    assert "match-device=type:wifi" in scan and "wlan0" not in scan
    guard = root / "usr/lib/openxplay-relay/clock-guard"
    subprocess.run(["sh", "-n", str(guard)], check=True)
    assert guard.stat().st_mode & 0o777 == 0o755
    assert "ExecStartPre=-/usr/lib/openxplay-relay/clock-guard" in (packaging / "openxplay-relay.service").read_text()
    # Exercise the public four-argument installer as packaged by release-linux.yml.
    release = Path(tmp) / "release"
    payload = release / "rust/packaging"
    payload.mkdir(parents=True)
    for name in ("install.sh", "openxplay-relay.service"):
        shutil.copyfile(packaging / name, payload / name)
    shutil.copytree(packaging / "host", payload / "host")
    for name in ("LICENSE", "LICENSE-GPL-3.0", "NOTICE"):
        (release / name).write_text("test license")
    for name in ("web/receiver/index.html", "admin/index.html", "bin/openxplay-relay", "bin/oxp-relay"):
        p = release / name
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text("test asset")
    commands = Path(tmp) / "commands"
    commands.mkdir()
    systemctl = commands / "systemctl"
    systemctl.write_text("#!/bin/sh\nexit 0\n")
    systemctl.chmod(0o755)
    full = Path(tmp) / "full"
    subprocess.run(["sh", str(payload / "install.sh"), str(release / "bin/openxplay-relay"),
                    str(release / "bin/oxp-relay"), str(release / "web"), str(release / "admin")],
                   env=dict(os.environ, DESTDIR=str(full), PATH=f"{commands}:{os.environ['PATH']}"),
                   check=True, stdout=subprocess.DEVNULL)
    for relative in ("etc/NetworkManager/conf.d/openxplay-relay-wifi-powersave.conf",
                     "etc/NetworkManager/conf.d/openxplay-relay-wifi-scan-mac.conf",
                     "usr/lib/openxplay-relay/clock-guard"):
        assert (full / relative).read_bytes() == (root / relative).read_bytes()
        assert (full / relative).stat().st_mode & 0o777 == (root / relative).stat().st_mode & 0o777
    legacy = Path(tmp) / "legacy/packaging"
    (legacy / "licenses").mkdir(parents=True)
    for name in ("install.sh", "openxplay-relay.service"):
        shutil.copyfile(packaging / name, legacy / name)
    for name in ("LICENSE", "LICENSE-GPL-3.0", "NOTICE"):
        shutil.copyfile(release / name, legacy / "licenses" / name)
    legacy_root = Path(tmp) / "legacy-root"
    subprocess.run(["sh", str(legacy / "install.sh"), str(release / "bin/openxplay-relay"),
                    str(release / "bin/oxp-relay"), str(release / "web"), str(release / "admin")],
                   env=dict(os.environ, DESTDIR=str(legacy_root), PATH=f"{commands}:{os.environ['PATH']}"),
                   check=True, stdout=subprocess.DEVNULL)
    assert (legacy_root / "opt/openxplay/bin/openxplay-relay").is_file()
    assert (legacy_root / "etc/systemd/system/openxplay-relay.service").is_file()
    for relative in ("etc/NetworkManager/conf.d/openxplay-relay-wifi-powersave.conf",
                     "etc/NetworkManager/conf.d/openxplay-relay-wifi-scan-mac.conf",
                     "usr/lib/openxplay-relay/clock-guard"):
        assert not (legacy_root / relative).exists()
    missing_host = subprocess.run(["sh", str(legacy / "install.sh"), "--install-host-tuning",
                                   str(Path(tmp) / "missing-host-root")], capture_output=True)
    assert missing_host.returncode != 0
    stub = Path(tmp) / "stub"
    stub.mkdir()
    env = dict(os.environ, PATH=f"{stub}:{os.environ['PATH']}", DESTDIR=str(root))

    def command(name, body):
        p = stub / name
        p.write_text("#!/bin/sh\n" + body + "\n")
        p.chmod(0o755)

    # macOS lacks coreutils timeout/stat; stubs let the same guard logic run there.
    real_timeout = shutil.which("timeout")
    if not real_timeout:
        command("timeout", 'shift 3; exec "$@"')
    command("systemctl", 'exit 0')
    command("timedatectl", 'echo "${SYNCED:-no}"')
    command("date", 'echo 100')
    command("stat", 'echo "${MTIME:-200}"')
    state = root / "var/lib/openxplay"
    tls = state / "receiver-tls"
    tls.mkdir(parents=True)
    files = [state / "admin-tls.json", tls / "cert.pem", tls / "key.pem"]
    identity = state / "mfi/identity.pk8"
    identity.parent.mkdir()
    identity.write_text("test-only-not-an-identity")
    for p in files:
        p.write_text("future TLS state")
    start = time.monotonic()
    subprocess.run(["sh", str(guard)], env=env, check=True)
    elapsed = time.monotonic() - start
    assert elapsed < 1.5, elapsed  # No unconditional sleep or NTP wait.
    assert all(p.exists() for p in files)
    env["SYNCED"] = "yes"
    env["MTIME"] = "50"
    subprocess.run(["sh", str(guard)], env=env, check=True)
    assert all(p.exists() for p in files)
    env["MTIME"] = "200"
    subprocess.run(["sh", str(guard)], env=env, check=True)
    assert all(not p.exists() for p in files)
    subprocess.run(["sh", str(guard)], env=env, check=True)  # Second guard is harmless.
    assert identity.read_text() == "test-only-not-an-identity"
    print(f"PASS staged policies, unit, full and legacy installs, strict missing-host failure, unsynced/current/future TLS, repeated guard; fast path {elapsed:.3f}s")
    if real_timeout:
        command("systemctl", "sleep 10")
        start = time.monotonic()
        subprocess.run(["sh", str(guard)], env=env, check=True, timeout=4)
        elapsed = time.monotonic() - start
        assert 1.8 < elapsed < 3, elapsed
        print(f"PASS stalled local command bounded: {elapsed:.3f}s")
    else:
        print("SKIP real GNU timeout deadline (coreutils timeout is unavailable)")
