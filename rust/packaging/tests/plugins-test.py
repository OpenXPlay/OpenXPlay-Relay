#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 OpenXPlay.org
# SPDX-License-Identifier: AGPL-3.0-only
# Created by riko chen on 2026-10-27

"""Optional bundle filtering and public installer regression checks, without root."""
import importlib.util
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import unittest

PACKAGING = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('bundle', PACKAGING / 'plugins-bundle.py')
bundle = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bundle)


class PluginsTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.source = self.root / 'bundle'
        self.source.mkdir()
        (self.source / 'host').mkdir()
        (self.source / 'host/index.html').write_text('host')

    def package(self, name, license_text='MIT'):
        package = self.source / 'p' / name
        package.mkdir(parents=True)
        (package / 'oxp-plugin.json').write_text(json.dumps({
            'spec': 'oxp-web-plugin/0', 'id': name, 'license': license_text}))
        (package / 'index.html').write_text(name)
        return package

    def test_android_selection_excludes_nc_invalid_and_symlink_packages(self):
        good = self.package('org.test.clock')
        for excluded in ('tests', 'tools', '.private'):
            (good / excluded).mkdir()
            (good / excluded / 'secret').write_text('excluded')
        (good / 'linked').symlink_to(self.source / 'host/index.html')
        self.package('org.test.nc', 'CC-BY-NC-4.0')
        self.package('org.test.noncommercial', 'NonCommercial')
        self.package('INVALID')
        invalid = self.package('org.test.invalid')
        (invalid / 'oxp-plugin.json').write_text('{}')
        (self.source / 'p/org.test.link').symlink_to(good, target_is_directory=True)
        destination = self.root / 'out'
        bundle.install(self.source, destination)
        self.assertEqual([p.name for p in (destination / 'p').iterdir()], ['org.test.clock'])
        self.assertEqual({p.name for p in (destination / 'p/org.test.clock').iterdir()}, {'index.html', 'oxp-plugin.json'})
        self.assertEqual((destination / 'host/index.html').read_text(), 'host')

    def installer(self, plugins=None):
        for relative in ('web/receiver/index.html', 'admin/index.html', 'relay', 'stream'):
            path = self.root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('payload')
        stub = self.root / 'tools'
        stub.mkdir(exist_ok=True)
        systemctl = stub / 'systemctl'
        systemctl.write_text('#!/bin/sh\nexit 0\n')
        systemctl.chmod(0o755)
        args = ['sh', str(PACKAGING / 'install.sh'), str(self.root / 'relay'), str(self.root / 'stream'), str(self.root / 'web'), str(self.root / 'admin')]
        if plugins is not None:
            args.append(str(plugins))
        return subprocess.run(args, env={**os.environ, 'DESTDIR': str(self.root / 'installed'), 'PATH': str(stub) + os.pathsep + os.environ['PATH']}, capture_output=True, text=True)

    def test_install_places_selected_bundle_at_the_runtime_plugins_root(self):
        self.package('org.test.clock')
        self.package('org.test.nc', 'CC-BY-NC-4.0')
        result = self.installer(self.source)
        self.assertEqual(result.returncode, 0, result.stderr)
        installed = self.root / 'installed/opt/openxplay/share/plugins'
        self.assertTrue((installed / 'host/index.html').is_file())
        self.assertEqual((installed / 'p/org.test.clock/index.html').stat().st_mode & 0o777, 0o644)
        self.assertFalse((installed / 'p/org.test.nc').exists())

    def test_no_bundle_keeps_the_original_four_argument_install(self):
        result = self.installer()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertFalse((self.root / 'installed/opt/openxplay/share/plugins').exists())

    def test_identity_or_missing_bundle_fails_before_installing_payload(self):
        (self.source / 'host/identity.pk8').write_text('identity')
        result = self.installer(self.source)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / 'installed/opt/openxplay/bin/relay').exists())
        self.assertNotEqual(self.installer(self.root / 'missing').returncode, 0)

    @unittest.skipUnless(shutil.which('dpkg-deb'), 'Debian package tools required')
    def test_deb_contains_the_same_filtered_optional_payload(self):
        self.package('org.test.clock')
        self.package('org.test.nc', 'CC-BY-NC-4.0')
        package = self.root / 'openxplay-relay-1.0.0-linux-aarch64'
        for relative in ('bin/openxplay-relay', 'bin/oxp-relay', 'web/receiver/index.html', 'admin/index.html'):
            path = package / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text('payload')
        for name in ('LICENSE', 'LICENSE-GPL-3.0', 'NOTICE'):
            shutil.copyfile(PACKAGING.parents[1] / name, package / name)
        unit = package / 'rust/packaging/openxplay-relay.service'
        unit.parent.mkdir(parents=True)
        shutil.copyfile(PACKAGING / 'openxplay-relay.service', unit)
        output = self.root / 'debs'
        subprocess.run(['sh', str(PACKAGING / 'build-deb.sh'), '--plugins-bundle', str(self.source), str(package), str(output)],
                       env={**os.environ, 'SOURCE_DATE_EPOCH': '1793000000'}, check=True, capture_output=True)
        deb = output / 'openxplay-relay_1.0.0_arm64.deb'
        subprocess.run(['dpkg-deb', '-x', str(deb), str(self.root / 'unpacked')], check=True)
        installed = self.root / 'unpacked/opt/openxplay/share/plugins'
        self.assertTrue((installed / 'host/index.html').is_file())
        self.assertTrue((installed / 'p/org.test.clock/index.html').is_file())
        self.assertFalse((installed / 'p/org.test.nc').exists())
        manifest = json.loads((output / 'openxplay-relay_1.0.0_arm64.manifest.json').read_text())
        self.assertEqual(manifest['requires']['dpkgArchitecture'], 'arm64')


if __name__ == '__main__':
    unittest.main()
