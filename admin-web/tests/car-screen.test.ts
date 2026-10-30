// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { expect, it } from 'vitest';
import { carNotes, carSelectionErrors, carScreenView, displayPresets, screenShare, screenShareText, trustedHttpsRecommended, viewAreasView } from '../src/car-screen';
import { fieldRoute } from '../src/settings-tree';
import { presentStage } from '../src/path/present-stage';
import { presentPath } from '../src/path/present-path';
import { esc } from '../src/html';
import { translate } from '../src/i18n';
import { linuxSettings, linuxStatusFixtures } from '../dev-server';
import type { Status } from '../src/api';
const screen = { width: 2401, height: 1201, densityDpi: 280 };
function fixture(): Status {
  const status = structuredClone(linuxStatusFixtures['live-browser']);
  status.screenData = { savedCanvas: { width: 1920, height: 960, densityDpi: 240, source: 'remembered' }, detected: screen, builtIn: true, connected: true, enabled: true,
    records: [
      { screen, size: { width: 1533, height: 1014 }, fullscreen: true, count: 3, lastSeen: 2 },
      { screen, size: { width: 1533, height: 1014 }, fullscreen: true, count: 5, lastSeen: 1 },
      { screen: { ...screen, densityDpi: 160 }, size: { width: 800, height: 600 }, fullscreen: false, count: 90, lastSeen: 3 },
    ], areas: [{ index: 0, width: 1920, height: 960, fullscreen: true, windowed: true }, { index: 1, width: 1450, height: 960, fullscreen: true, windowed: false }], active: 1, preference: true,
    switches: [{ time: 1000, index: 1, accepted: true }, { time: 2000, index: 2, accepted: false }] };
  status.receiverHistory = { screens: [{ ...screen, info: { userAgent: 'browser', token: 'secret-token', url: 'private-url', ssid: 'private-network',
    videoDecode: [{ codec: 'avc1.640033', supported: true, token: 'hidden', fps60: { smooth: true, powerEfficient: false, token: 'hidden' } }], audioDecoder: [{ codec: 'aac', supported: true }] } }],
    fullscreenExits: [{ time: 1000, client: '<client>', report: { hiddenMs: 120, autoRestore: 'denied', display: { viewport: { width: 1500, height: 900 } } } }] };
  return status;
}
it('requires brand and model including Other text, while year is optional', () => {
  expect(carSelectionErrors({})).toEqual({ carBrand: 'car.brandRequired', carModel: 'car.modelRequired' });
  expect(carSelectionErrors({ carBrand: 'other', carBrandText: ' ', carModel: 'other', carModelText: ' ' })).toHaveProperty('carModel');
  expect(carSelectionErrors({ carBrand: 'other', carBrandText: 'A', carModel: 'other', carModelText: 'B', carYear: 'other', carYearText: '' })).toEqual({});
  expect(carSelectionErrors({ carBrand: 'zeekr', carModel: '001' })).toEqual({});
});
it.each(['en', 'zh-CN'] as const)('offers the Zeekr recommendation and stage notice until HTTPS is configured (%s)', language => {
  const t = (key: string, args?: Record<string, string | number>) => translate(language, key, args);
  const values = { carBrand: 'zeekr', carModel: '001' };
  expect(trustedHttpsRecommended(values)).toBe(true);
  expect(carNotes(values, t)).toContain('#settings/access/public-name');
  expect(carNotes(values, t)).toContain(esc(t('car.zeekrHttps')));
  expect(trustedHttpsRecommended({ ...values, publicNameEnabled: true, publicName: 'car.example.org', publicNameProvider: 'dynv6', publicNameTokenSet: true, publicNameAgreeTerms: true })).toBe(false);
  expect(trustedHttpsRecommended({ ...values, carBrand: 'tesla' })).toBe(false);
  const path = presentPath(fixture().pathInput!);
  expect(presentStage(path, { trustedHttpsRecommended: true }).notice).toBe('TRUSTED_HTTPS');
  expect(presentStage(path, { carIncomplete: true, trustedHttpsRecommended: true }).notice).toBe('TRUSTED_HTTPS');
  expect(presentStage(path, { displayChanged: true, trustedHttpsRecommended: true }).notice).toBe('DISPLAY_CHANGED');
});
it('shares only Android format-2 car, matching screen observations and allowlisted capabilities', () => {
  const status = fixture(), settings = structuredClone(linuxSettings);
  Object.assign(settings.values, { carBrand: 'other', carBrandText: 'My brand', carModel: 'other', carModelText: 'My model', carYear: 'other', carYearText: '2027', publicNameToken: 'secret-token' });
  const share = screenShare(status, settings)!;
  expect(share).toMatchObject({ format: 2, app: status.version, brandText: 'My brand', modelText: 'My model', yearText: '2027', screen, windowed: [] });
  expect(share.fullscreen).toEqual([{ width: 1533, height: 1014, count: 5 }, { width: 1533, height: 1014, count: 3 }]);
  expect(share.videoDecode).toEqual([{ codec: 'avc1.640033', supported: true, fps60: { smooth: true, powerEfficient: false } }]);
  const t = (key: string, args?: Record<string, string | number>) => translate('en', key, args);
  const text = screenShareText(status, settings, 'en', t);
  expect(text).toContain('My brand'); expect(text).toContain('2027');
  expect(text).not.toMatch(/token|secret|private-url|private-network|client|lastSeen/);
  status.screenData!.detected = null;
  expect(screenShare(status, settings)).toBeNull();
});
it.each(['en', 'zh-CN'] as const)('shows saved source, density, visits, area controls and escaped exit history (%s)', language => {
  const t = (key: string, args?: Record<string, string | number>) => translate(language, key, args);
  const status = fixture();
  const screenHtml = carScreenView(status, t), areas = viewAreasView(status, t);
  expect(screenHtml).toContain('240 dpi'); expect(screenHtml).toContain('1.75'); expect(screenHtml).toContain(t('car.source.remembered'));
  expect(screenHtml).toContain('data-screen-action="clear-saved-head-unit"');
  expect(screenHtml).toContain(t('car.windowSize', { width: 1533, height: 1014, count: 5 }));
  expect(areas).toContain('data-show-area="0"'); expect(areas).toContain('data-screen-action="clear-window-records"');
  expect(areas).toContain('1500 × 900'); expect(areas).toContain('120 ms'); expect(areas).toContain('&lt;client&gt;'); expect(areas).toContain(t('areas.denied'));
  status.screenData!.enabled = false;
  expect(viewAreasView(status, t)).not.toContain('data-show-area');
  status.screenData!.detected = null;
  expect(carScreenView(status, t)).toContain(t('car.noScreen'));
});
it('places view areas in Car, Carlink sizes in Picture, and calibration in Developer/Carlink', () => {
  expect(fieldRoute('carPlayViewAreas')).toBe('settings/car');
  expect(fieldRoute('carYearText')).toBe('settings/car');
  expect(fieldRoute('carlinkPictureSizes')).toBe('settings/picture');
  expect(fieldRoute('displayDensity')).toBe('settings/developer/video');
  expect(fieldRoute('physicalWidthOverrideMm')).toBe('settings/developer/carlink');
  expect(displayPresets).toEqual([[800, 480, 160], [1024, 600, 160], [1280, 720, 160], [1920, 720, 240], [1920, 1080, 240]]);
});
