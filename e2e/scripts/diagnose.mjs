#!/usr/bin/env node
//! Diagnóstico mínimo: ¿arranca la app?

import { appBinary } from '../lib/env.mjs';
import { connect, disconnect, getBrowser, screenshot, startDriver, stopDriver } from '../lib/session.mjs';

await startDriver();
await connect();
const b = getBrowser();
await b.pause(4000);
console.log('title:', await b.getTitle());
console.log('url:', await b.getUrl());
console.log(
  'estado:',
  await b.execute(() => ({
    ready: document.readyState,
    tabs: document.querySelectorAll('[role="tab"]').length,
    text: (document.body?.innerText ?? '').slice(0, 200)
  }))
);
console.log('shot:', await screenshot('diagnostico'));
await disconnect();
stopDriver();
console.log('binario:', appBinary());
