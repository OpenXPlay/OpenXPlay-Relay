#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-27

"""Copy an optional prebuilt install bundle using Android's package selection rules."""
import json
import re
import shutil
import sys
from pathlib import Path


def copy_tree(source, destination):
    if source.is_symlink():
        return
    if source.is_dir():
        destination.mkdir(parents=True, exist_ok=True)
        for child in sorted(source.iterdir()):
            if not child.name.startswith('.') and child.name not in ('tests', 'tools'):
                copy_tree(child, destination / child.name)
    elif source.is_file():
        if source.name in ('identity.pk8', 'certificate.p7b') or source.suffix in ('.p12', '.pfx'):
            raise ValueError('Refusing plugin bundle containing identity material')
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(source, destination)


def install(source, destination):
    source, destination = Path(source), Path(destination)
    if not source.is_dir() or source.is_symlink():
        raise ValueError('Plugins bundle must be an install directory')
    # Validate in staging before touching the installation.
    copy_tree(source / 'host', destination / 'host')
    packages = source / 'p'
    if not packages.is_dir() or packages.is_symlink():
        return
    for package in sorted(packages.iterdir()):
        manifest = package / 'oxp-plugin.json'
        if (package.is_symlink() or not package.is_dir() or len(package.name) > 64
                or not re.fullmatch(r'[a-z0-9]+(\.[a-z0-9-]+)+', package.name)
                or manifest.is_symlink() or not manifest.is_file() or manifest.stat().st_size > 65536):
            continue
        try:
            value = json.loads(manifest.read_text(encoding='utf-8'))
            license_text = value.get('license')
            if (value.get('spec') != 'oxp-web-plugin/0' or value.get('id') != package.name
                    or not isinstance(license_text, str) or not license_text.strip()
                    or set(re.split('[^A-Z0-9]+', license_text.upper())) & {'NC', 'NONCOMMERCIAL'}):
                continue
        except (ValueError, UnicodeError, AttributeError):
            continue
        copy_tree(package, destination / 'p' / package.name)


if __name__ == '__main__':
    try:
        install(*sys.argv[1:])
    except (ValueError, OSError, TypeError) as error:
        sys.exit(str(error))
