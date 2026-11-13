#!/usr/bin/env node
//! Arranca tauri-driver, crea el fixture con git real, ejecuta Mocha y limpia.

import { readdirSync, rmSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import Mocha from 'mocha';

import { ARTIFACTS, E2E_DIR, E2E_LOCALE, appBinary, artifactPath, isolateAppState, tauriDriverBin } from '../lib/env.mjs';
import { cleanupFixtures, createRepoFixture } from '../lib/fixture.mjs';
import { normalizeStart } from '../lib/flows.mjs';
import {
  assertPackaged,
  connect,
  disconnect,
  getBrowser,
  screenshot,
  startDriver,
  stopDriver
} from '../lib/session.mjs';
import { setFixture } from '../lib/state.mjs';

const TEST_DIR = path.join(E2E_DIR, 'test');

function parseArgs(argv) {
  const out = { grep: undefined, bail: false };
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === '--grep') out.grep = argv[++i];
    else if (argv[i] === '--bail') out.bail = true;
  }
  return out;
}

async function waitForApp() {
  const b = getBrowser();
  await assertPackaged();
  await b.waitUntil(
    async () =>
      b.execute((locale) => {
        const ready = document.readyState === 'complete';
        const shell = !!document.querySelector('header, [role="tablist"]');
        const booting = !!document.querySelector('[aria-busy="true"]');
        // `<html lang>` lo pone la app al aplicar el idioma guardado en `settings.json`.
        const lang = (document.documentElement.lang ?? '').toLowerCase();
        return ready && shell && !booting && lang.startsWith(locale);
      }, E2E_LOCALE),
    { timeout: 60_000, interval: 250, timeoutMsg: `la ventana de Ariadna no llegó a estar lista en «${E2E_LOCALE}»` }
  );
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const started = Date.now();

  console.log('· binario:', appBinary());
  console.log('· tauri-driver:', tauriDriverBin());

  cleanupFixtures();
  isolateAppState();
  // Las capturas de ejecuciones anteriores no deben mezclarse con las de esta.
  rmSync(path.join(ARTIFACTS, 'screens'), { recursive: true, force: true });
  const fixture = createRepoFixture();
  setFixture(fixture);
  console.log('· fixture:', fixture.repo, `(${fixture.commitsTotales} commits)`);

  await startDriver();
  await connect();
  await waitForApp();
  await normalizeStart();

  const mocha = new Mocha({
    timeout: Number(process.env.E2E_TEST_TIMEOUT ?? 120_000),
    slow: Number(process.env.E2E_SLOW ?? 10_000),
    reporter: process.env.E2E_REPORTER ?? 'spec',
    grep: args.grep,
    bail: args.bail,
    color: process.stdout.isTTY === true,
    rootHooks: {
      afterEach: async function () {
        const title = this?.currentTest?.title ?? 'prueba';
        const state = this?.currentTest?.state;
        await screenshot(`${state === 'passed' ? 'ok' : 'fallo'}-${title}`);
      }
    }
  });

  const files = readdirSync(TEST_DIR)
    .filter((f) => f.endsWith('.e2e.mjs'))
    .sort();
  for (const f of files) mocha.addFile(path.join(TEST_DIR, f));

  const runner = mocha.run();
  const stats = await new Promise((resolve) => runner.once('end', () => resolve(runner.stats)));
  const failures = stats.failures;
  const seconds = ((Date.now() - started) / 1000).toFixed(1);

  await disconnect();
  stopDriver();

  const total = stats.tests;
  const passed = stats.passes;
  console.log('\n──────── resumen E2E ────────');
  console.log(`flujos: ${total} · pasan: ${passed} · fallan: ${failures} · omitidos: ${stats.pending}`);
  console.log(`duración total: ${seconds} s · reloj de pruebas: ${(stats.duration / 1000).toFixed(1)} s`);
  console.log(`binario: ${appBinary()}`);
  console.log(`capturas y logs: ${artifactPath('.')}`);

  cleanupFixtures();
  process.exit(failures === 0 ? 0 : 1);
}

main().catch(async (err) => {
  console.error('\n✖ la suite no pudo arrancar:', err?.message ?? err);
  try {
    await disconnect();
  } catch {
    /* nada */
  }
  stopDriver();
  cleanupFixtures();
  process.exit(2);
});
