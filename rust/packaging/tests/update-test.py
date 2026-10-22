#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-21
"""Unprivileged updater checks with fake commands and a monotonic test clock."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

packaging = Path(__file__).resolve().parents[1]
with tempfile.TemporaryDirectory() as tmp:
    root = Path(tmp)
    commands = root / "commands"
    commands.mkdir()
    dispatch = commands / "dispatch"
    dispatch.write_text(f"#!{sys.executable}\n" + r'''
import hashlib, json, os, sys
from pathlib import Path
state_file = Path(os.environ["TEST_STATE"])
s = json.loads(state_file.read_text())
name, args = Path(sys.argv[0]).name, sys.argv[1:]
s["calls"].append([name, args])
rc = 0
if name == "id":
    print(s.get("uid", 0))
elif name == "lslocks":
    if s.get("probe_error"):
        rc = 1
    else:
        print(s.get("other_locks", "/tmp/unrelated-lock"))
        if s["busy"]:
            print("/var/lib/dpkg/" + s.get("lock", "lock-frontend"))
elif name == "sleep":
    seconds = float(args[0])
    s["now"] += seconds
    if seconds and s["busy"] > 0:
        s["busy"] -= 1
    Path(os.environ["TEST_UPTIME"]).write_text(f'{s["now"]} 0\n')
elif name == "dpkg-query":
    if s["installed"]:
        print("install ok installed " + s["installed"])
    else:
        rc = 1
elif name == "dpkg-deb":
    if s.get("metadata_error") and args[-1] == "Version":
        print("dpkg-deb: invalid archive", file=sys.stderr)
        rc = 2
    else:
        print(json.loads(Path(args[1]).read_text())[args[2]])
elif name == "dpkg" and args[0] == "--compare-versions":
    a, op, b = args[1:]
    a, b = tuple(map(int, a.split("."))), tuple(map(int, b.split(".")))
    rc = 0 if (a < b if op == "lt" else a > b) else 1
elif name == "dpkg":
    version = json.loads(Path(args[-1]).read_text())["Version"]
    s["attempts"].append(version)
    if s["busy"]:
        print("dpkg invoked while a known lock was held", file=sys.stderr)
        rc = 99
    elif s.get("race", 0):
        s["race"] -= 1 if s["race"] > 0 else 0
        print(s.get("lock_error", "dpkg: error: dpkg frontend lock was locked by another process with pid 123"), file=sys.stderr)
        rc = 2
    elif version in s.get("dpkg_fail", []):
        print("dpkg: error: fixture installation failed", file=sys.stderr)
        rc = 1
    else:
        s["installed"] = version
        s["busy"] = s.get("busy_after", {}).get(version, 0)
        print("dpkg installed " + version)
        if s.get("warning"):
            print("dpkg: warning: fixture warning", file=sys.stderr)
elif name == "systemctl":
    if args[0] == "is-enabled":
        rc = 0 if s.get("enabled", True) else 1
    elif args[0] == "show":
        prop = args[2]
        print({"ActiveState": "failed" if s["installed"] in s.get("bad", []) else "active",
               "MainPID": "4242", "NRestarts": "0"}[prop])
elif name == "sha256sum":
    print(hashlib.sha256(Path(args[0]).read_bytes()).hexdigest() + "  " + args[0])
elif name == "sort":
    print("\n".join(sorted(sys.stdin.read().splitlines(),
                          key=lambda v: tuple(map(int, v.split("."))), reverse=True)))
else:
    raise AssertionError((name, args))
state_file.write_text(json.dumps(s))
sys.exit(rc)
''')
    dispatch.chmod(0o755)
    for name in ("id", "lslocks", "sleep", "dpkg-query", "dpkg-deb", "dpkg", "systemctl", "sha256sum", "sort"):
        (commands / name).symlink_to(dispatch)
    # Change only the uptime input so the real polling loop runs without waiting 180 seconds.
    # This also lets these fake-command tests run on macOS, which has no /proc/uptime.
    source = (packaging / "openxplay-relay-update.sh").read_text()
    assert source.count("< /proc/uptime") == 1
    update = root / "update.sh"
    update.write_text(source.replace("< /proc/uptime", '< "$TEST_UPTIME"'))
    count = 0

    def run(*args, current="", cached=(), timeout=None, **settings):
        global count
        count += 1
        case = root / str(count)
        case.mkdir()
        cache = case / "cache"
        cache.mkdir()
        metadata = {"Package": "openxplay-relay", "Architecture": "arm64", "Version": "2.0"}
        deb = case / "new.deb"
        deb.write_text(json.dumps(metadata))
        for version in cached:
            (cache / f"openxplay-relay_{version}_arm64.deb").write_text(json.dumps(dict(metadata, Version=version)))
        state = case / "state.json"
        initial = dict(installed=current, busy=0, now=100.9, calls=[], attempts=[])
        initial.update(settings)
        state.write_text(json.dumps(initial))
        uptime = case / "uptime"
        uptime.write_text("100.9 0\n")
        env = dict(os.environ, PATH=f"{commands}:{os.environ['PATH']}", TEST_STATE=str(state),
                   TEST_UPTIME=str(uptime), OPENXPLAY_RELAY_CACHE=str(cache),
                   OPENXPLAY_RELAY_UPDATE_TIMEOUT="0", OPENXPLAY_RELAY_UPDATE_SETTLE="0")
        env.pop("OPENXPLAY_RELAY_UPDATE_LOCK_TIMEOUT", None)
        if timeout is not None:
            env["OPENXPLAY_RELAY_UPDATE_LOCK_TIMEOUT"] = str(timeout)
        if settings.get("cache_error"):
            cache.rmdir()
            cache.write_text("not a directory")
        result = subprocess.run(["sh", str(update), *(args or [str(deb)])], env=env,
                                capture_output=True, text=True, timeout=30)
        final = json.loads(state.read_text())
        assert "OXP-STAGE relay failed:" not in result.stdout, result
        return result, final, cache

    def failed(result, message):
        assert result.returncode == 1, result
        assert result.stderr.count("OXP-STAGE relay failed:") == 1, result
        assert f"OXP-STAGE relay failed: {message}\n" in result.stderr, result

    def waits(result):
        return result.stdout.count("OXP-STAGE relay waiting: dpkg is busy (")

    result, state, cache = run(warning=True)
    assert result.returncode == 0 and state["installed"] == "2.0" and waits(result) == 0, result
    assert "dpkg: warning: fixture warning" in result.stderr
    assert "OXP-STAGE relay ok version=2.0 previous=none" in result.stdout
    for lock in ("lock-frontend", "lock"):
        result, state, cache = run(busy=3, lock=lock)
        assert result.returncode == 0 and waits(result) == 1 and state["attempts"] == ["2.0"], result
        assert state["now"] == 103.9 and state["installed"] == "2.0", state

    result, state, cache = run(busy=-1, current="1.0", cached=("1.0",))
    failed(result, "dpkg is locked by another process")
    assert waits(result) == 1 and not state["attempts"] and state["installed"] == "1.0"
    assert sum(call[0] == "sleep" for call in state["calls"]) == 180
    result, state, cache = run(busy=-1, lock="lock", timeout=2)
    failed(result, "dpkg is locked by another process")
    assert not state["attempts"] and waits(result) == 1
    assert sum(call[0] == "sleep" for call in state["calls"]) == 2

    result, state, cache = run("--rollback", current="2.0", cached=("1.0", "2.0"), busy=2)
    assert result.returncode == 0 and waits(result) == 1 and state["attempts"] == ["1.0"], result
    assert "OXP-STAGE relay ok version=1.0 rolled-back-from=2.0" in result.stdout
    result, state, cache = run("--rollback", current="2.0", cached=("1.0", "2.0"), busy=-1, timeout=2)
    failed(result, "dpkg is locked by another process")
    assert not state["attempts"] and state["installed"] == "2.0"

    result, state, cache = run(current="1.0", cached=("1.0",), bad=["2.0"], busy_after={"2.0": 3})
    assert result.returncode == 2 and waits(result) == 1 and state["attempts"] == ["2.0", "1.0"], result
    assert "OXP-STAGE relay rolled-back version=1.0 failed=2.0" in result.stdout
    assert "OXP-STAGE relay failed:" not in result.stderr
    assert not (cache / "openxplay-relay_2.0_arm64.deb").exists()
    result, state, cache = run(current="1.0", cached=("1.0",), bad=["2.0"], busy_after={"2.0": -1}, timeout=2)
    failed(result, "dpkg is locked by another process")
    assert state["attempts"] == ["2.0"] and state["installed"] == "2.0"

    for error in ("dpkg: error: dpkg frontend lock was locked by another process with pid 123",
                  "dpkg: error: dpkg database lock is locked by another process"):
        result, state, cache = run(race=2, lock_error=error)
        assert result.returncode == 0 and waits(result) == 1 and state["attempts"] == ["2.0"] * 3, result
    result, state, cache = run(race=-1, timeout=2)
    failed(result, "dpkg is locked by another process")
    assert waits(result) == 1 and state["installed"] == ""

    result, state, cache = run(dpkg_fail=["2.0"])
    failed(result, "dpkg failed for 2.0 and there is nothing to roll back to")
    assert "dpkg: error: fixture installation failed" in result.stderr and state["attempts"] == ["2.0"]
    result, state, cache = run(current="1.0", cached=("1.0",), dpkg_fail=["2.0"])
    assert result.returncode == 2 and state["attempts"] == ["2.0", "1.0"], result
    assert "OXP-STAGE relay failed:" not in result.stderr
    result, state, cache = run("--rollback", current="2.0", cached=("1.0",), dpkg_fail=["1.0"])
    failed(result, "dpkg failed for 1.0")
    result, state, cache = run(current="1.0", cached=("1.0",), bad=["2.0"], dpkg_fail=["1.0"])
    failed(result, "rollback to 1.0 failed")
    result, state, cache = run(current="1.0", cached=("1.0",), bad=["1.0", "2.0"])
    failed(result, "relay not healthy after rolling back to 1.0")

    for args, settings, message in (
        (["--unknown"], {}, "unknown option --unknown"),
        (["--keep"], {}, "--keep needs a number"),
        (["--sha256"], {}, "--sha256 needs a hash"),
        (["--keep", "bad"], {}, "--keep needs a number"),
        (["--rollback"], {}, "openxplay-relay is not installed as a package"),
        ([], {"uid": 1000}, "run as root"),
        ([], {"probe_error": True}, "cannot inspect dpkg locks"),
        ([], {"metadata_error": True}, "update exited with status 2"),
        ([], {"cache_error": True}, "update exited with status 1"),
    ):
        result, state, cache = run(*args, **settings)
        failed(result, message)
        assert not state["attempts"]
    print(f"PASS {count} updater scenarios (including the default 180-second lock deadline)")
