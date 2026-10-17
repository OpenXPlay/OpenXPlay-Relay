// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { gzipSync } from 'node:zlib';
import { defineConfig } from 'vitest/config';
import { mockApi } from './dev-server';
export default defineConfig({
  base: './',
  cacheDir: '.vite',
  plugins: [mockApi(process.env.DIRELAY_MOCK_LINUX === '1'), {
    name: 'admin-bundle-budget',
    generateBundle: { order: 'post', handler(_options, bundle) {
      const bytes = Object.values(bundle).reduce((sum, file) => sum + gzipSync(file.type === 'chunk' ? file.code : file.source).byteLength, 0);
      if (bytes > 60 * 1024) this.error(`Admin gzip budget exceeded: ${bytes} bytes`);
      this.info(`Admin gzip: ${bytes} / ${60 * 1024} bytes`);
    } },
  }],
  build: { outDir: 'dist' },
  test: { environment: 'node', include: ['tests/**/*.test.ts'] },
});
