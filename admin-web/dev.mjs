// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

import { createServer } from 'vite';
if (process.argv.includes('--linux')) process.env.DIRELAY_MOCK_LINUX = '1';
if (process.argv.includes('--states')) { process.env.DIRELAY_PREVIEW_STATES = '1'; process.env.DIRELAY_MOCK_LINUX = '1'; }
const server = await createServer({ configLoader: 'runner', server: { host: '127.0.0.1' } });
await server.listen();
server.printUrls();
if (process.argv.includes('--states')) console.info('Stage gallery: /states (Linux and Android; stage, startup, repair and feature states).');
for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, async () => { await server.close(); process.exit(0); });
