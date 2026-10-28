// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-27

import { describe, expect, it } from 'vitest';
import { capabilityFieldEnabled, capabilityFieldVisible, capabilityView } from '../src/capabilities';
import type { Status } from '../src/api';
import { fieldEnabled, fieldRoute } from '../src/settings-tree';
import { translate } from '../src/i18n';

describe('Linux optional capabilities', () => {
  const empty = () => ({ platform: 'linux' } as Status);
  it('hides encoder-dependent choices on decoder-only devices and follows capability updates', () => {
    const status = empty();
    for (const key of ['webTranscode', 'carlinkHevc', 'carlinkPictureSize']) expect(capabilityFieldVisible(key, status)).toBe(false);
    expect(capabilityFieldVisible('playCarPlayAudio', status)).toBe(true);
    const cap = { enabled: true, available: true, decode: ['H264'], encode: [{ codec: 'H264', maxWidth: null, maxHeight: null }, { codec: 'H265', maxWidth: null, maxHeight: null }], scaling: true, description: 'encoder' };
    status.transcoding = { web: cap, clink: cap };
    for (const key of ['webTranscode', 'carlinkHevc', 'carlinkPictureSize']) expect(capabilityFieldVisible(key, status)).toBe(true);
    cap.available = false;
    expect(capabilityFieldVisible('carlinkHevc', status)).toBe(false);
    expect(capabilityFieldVisible('carlinkHevc', { platform: 'android' } as Status)).toBe(true);
  });
  it('disables encoder and ALSA options until an appropriate device is reported', () => {
    const status = empty();
    for (const key of ['webTranscode', 'carlinkHevc', 'carlinkPictureSize', 'playCarPlayAudio', 'microphoneSource']) expect(capabilityFieldEnabled(key, status)).toBe(false);
    const cap = { enabled: true, available: true, decode: ['H264'], encode: [{ codec: 'H264', maxWidth: null, maxHeight: null }], scaling: true, description: '<encoder>' };
    status.transcoding = { web: cap, clink: cap };
    expect(capabilityFieldEnabled('webTranscode', status)).toBe(true);
    expect(capabilityFieldEnabled('carlinkPictureSize', status)).toBe(true);
    expect(capabilityFieldEnabled('carlinkHevc', status)).toBe(false);
    cap.encode[0].codec = 'H265';
    expect(capabilityFieldEnabled('carlinkHevc', status)).toBe(true);
    expect(capabilityFieldEnabled('carlinkPictureSize', status)).toBe(false);
    expect(capabilityFieldEnabled('webTranscode', status)).toBe(false);
    status.localAudio = { playbackAvailable: true, captureAvailable: false, playback: ['plughw:0,0'], capture: [] };
    expect(capabilityFieldEnabled('playCarPlayAudio', status)).toBe(true);
    expect(capabilityFieldEnabled('microphoneSource', status)).toBe(false);
    expect(capabilityView(status, key => translate('en', key))).toContain('&lt;encoder&gt;');
  });
  it('keeps choices under the developer master and places them on Android-equivalent pages', () => {
    for (const key of ['webTranscode', 'playCarPlayAudio', 'microphoneSource', 'microphoneSourceEnabled']) expect(fieldEnabled(key, { developerActive: false })).toBe(false);
    expect(fieldEnabled('microphoneSource', { developerActive: true, microphoneSourceEnabled: false })).toBe(false);
    expect(fieldRoute('microphoneSource')).toBe('settings/developer/connection');
    expect(fieldRoute('playCarPlayAudio')).toBe('settings/developer/connection');
    expect(fieldRoute('webTranscode')).toBe('settings/developer/video');
    expect(capabilityFieldEnabled('webTranscode', { platform: 'android' } as Status)).toBe(true);
    expect(capabilityView({ platform: 'android' } as Status, key => key)).toBe('');
  });
});
