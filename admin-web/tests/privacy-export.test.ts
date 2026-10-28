// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { afterEach, expect, it, vi } from 'vitest';
import { api } from '../src/api';
import { mountFeaturePage } from '../src/feature-pages';
import { translate } from '../src/i18n';
import { mockSettings } from '../dev-server';

// A small EventTarget host exercises the page's confirmation flow without a browser dependency.
class Host extends EventTarget {
  innerHTML = ''; textContent = ''; disabled = false; hidden = false; isConnected = true;
  href = ''; download = ''; children = new Map<string, Host>();
  querySelector(selector: string): Host {
    if (!this.children.has(selector)) this.children.set(selector, new Host());
    return this.children.get(selector)!;
  }
  toggleAttribute(name: string, value: boolean) { if (name === 'hidden') this.hidden = value; }
  setAttribute() {} showModal() {} close() {} remove() {} focus() {}
  click() { this.dispatchEvent(new Event('click')); }
}
afterEach(() => { vi.restoreAllMocks(); vi.unstubAllGlobals(); });
it.each(['en', 'zh-CN'] as const)('requires a fresh confirmation for each privacy download (%s)', async language => {
  const settings = mockSettings(true); settings.values.privacyConfigAvailable = true;
  vi.spyOn(api, 'settings').mockResolvedValue(settings);
  const dialogs: Host[] = [], downloads: Host[] = [];
  vi.stubGlobal('document', {
    body: { append: (node: Host) => dialogs.push(node) },
    createElement: (tag: string) => {
      const node = new Host();
      if (tag === 'a') node.click = () => { downloads.push(node); };
      return node;
    },
  });
  const container = new Host(); container.querySelector('#privacy-export').disabled = true;
  mountFeaturePage(container as unknown as HTMLElement, 'settings/config-files', {
    t: (key, parameters) => translate(language, key, parameters), language,
    version: 'test', platform: 'linux', adminFeatures: ['config-files'],
    setLanguage: () => {}, error: () => 'error.request', reload: async () => {},
  });
  await vi.waitFor(() => expect(container.querySelector('#privacy-export').disabled).toBe(false));
  expect(container.innerHTML).not.toContain('href="/api/v0/config/privacy"');
  container.querySelector('#privacy-export').click();
  expect(downloads).toHaveLength(0);
  expect(dialogs[0].innerHTML).toContain(language === 'en' ? 'Anyone with the file can use your HTTPS name.' : '拿到文件的人可以使用你的 HTTPS 域名。');
  dialogs[0].querySelector('[data-close]').click();
  expect(downloads).toHaveLength(0);
  container.querySelector('#privacy-export').click();
  dialogs[1].querySelector('[data-export]').click();
  expect(downloads.map(node => [node.href, node.download])).toEqual([['/api/v0/config/privacy', 'openxplay-privacy-config.json']]);
  container.querySelector('#privacy-export').click();
  expect(dialogs).toHaveLength(3);
  expect(downloads).toHaveLength(1);
});
it('disables privacy export and explains the empty state without retrieving secrets', async () => {
  vi.spyOn(api, 'settings').mockResolvedValue(mockSettings(true));
  const container = new Host(); container.querySelector('#privacy-export').disabled = true;
  mountFeaturePage(container as unknown as HTMLElement, 'settings/config-files', {
    t: key => translate('en', key), language: 'en', version: 'test', platform: 'linux',
    adminFeatures: ['config-files'], setLanguage: () => {}, error: () => 'error.request', reload: async () => {},
  });
  await vi.waitFor(() => expect(container.querySelector('#privacy-export').disabled).toBe(true));
  expect(container.querySelector('#privacy-empty').hidden).toBe(false);
  expect(container.innerHTML).toContain('No private data to export.');
});
