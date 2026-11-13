#!/usr/bin/env node
//! Regresión visual: captura 8 pantallas en claro y oscuro y las compara con
//! los baselines de `e2e/baselines/<so>/`. `--update` regenera los baselines.

import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import { E2E_DIR, appBinary, artifactPath, git, isolateAppState, platformTag } from '../lib/env.mjs';
import { cleanupFixtures, createRepoFixture } from '../lib/fixture.mjs';
import { ensureRepoOpen, normalizeStart } from '../lib/flows.mjs';
import { assertPackaged, connect, disconnect, getBrowser, startDriver, stopDriver } from '../lib/session.mjs';
import { getFixture, setFixture } from '../lib/state.mjs';
import { MAX_DIFF_RATIO, PIXEL_THRESHOLD, comparePng, writePng } from './compare.mjs';
import { screens } from './screens.mjs';

const THEMES = ['light', 'dark'];

function parseArgs(argv) {
  const out = { update: false, only: null, platform: null };
  for (const arg of argv) {
    if (arg === '--update') out.update = true;
    else if (arg.startsWith('--only=')) out.only = arg.slice('--only='.length).split(',');
    else if (arg.startsWith('--platform=')) out.platform = arg.slice('--platform='.length);
  }
  return out;
}

/** Fija el tema y congela transiciones para que la captura sea determinista. */
async function freeze(theme) {
  const b = getBrowser();
  await b.execute((t) => {
    document.documentElement.setAttribute('data-theme', t);
    let style = document.getElementById('e2e-freeze');
    if (!style) {
      style = document.createElement('style');
      style.id = 'e2e-freeze';
      document.head.appendChild(style);
    }
    style.textContent =
      '*,*::before,*::after{transition:none!important;animation:none!important;caret-color:transparent!important}';
  }, theme);
  await b.pause(120);
}

async function capture() {
  const b = getBrowser();
  return Buffer.from(await b.takeScreenshot(), 'base64');
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.platform) process.env.E2E_PLATFORM = args.platform;
  const platform = platformTag();
  const baselineDir = path.join(E2E_DIR, 'baselines', platform);
  const diffDir = path.join(E2E_DIR, 'artifacts', 'visual', platform);

  const selected = args.only ? screens.filter((s) => args.only.includes(s.name)) : screens;
  if (selected.length === 0) throw new Error(`ninguna pantalla coincide con --only=${args.only?.join(',')}`);

  console.log(`· plataforma: ${platform}`);
  console.log(`· binario: ${appBinary()}`);
  console.log(`· pantallas: ${selected.map((s) => s.name).join(', ')}`);
  console.log(`· modo: ${args.update ? 'ACTUALIZAR baselines' : 'comparar'} · umbral ${PIXEL_THRESHOLD}, tolerancia ${(MAX_DIFF_RATIO * 100).toFixed(1)} %`);

  cleanupFixtures();
  isolateAppState();
  setFixture(createRepoFixture({ name: 'repo-visual' }));

  await startDriver();

  const results = [];
  if (!args.update) rmSync(diffDir, { recursive: true, force: true });

  for (const theme of THEMES) {
    // Cada tema arranca la app de cero: las pantallas que preparan archivos dejan
    // el panel de diferencias esperando contenido y contaminarían el siguiente tema.
    await connect();
    await assertPackaged();
    await normalizeStart();
    await ensureRepoOpen();
    await getBrowser().pause(400);
    await freeze(theme);
    for (const screen of selected) {
      const key = `${theme}-${screen.name}`;
      try {
        await screen.prepare();
        await freeze(theme);
        const png = await capture();
        const actualPath = artifactPath('visual', platform, `actual-${key}.png`);

        if (args.update) {
          const target = path.join(baselineDir, `${key}.png`);
          writePng(target, png);
          results.push({ key, status: 'baseline', detail: target });
        } else {
          writePng(actualPath, png);
          const baselinePath = path.join(baselineDir, `${key}.png`);
          if (!existsSync(baselinePath)) {
            results.push({ key, status: 'sin baseline', detail: `genera con --update (${baselinePath})` });
            continue;
          }
          const r = comparePng(png, baselinePath, path.join(diffDir, `${key}.diff.png`));
          results.push({ key, status: r.ok ? 'ok' : 'difiere', detail: r.message });
        }
      } catch (err) {
        results.push({ key, status: 'error', detail: err?.message?.split('\n')[0] ?? String(err) });
      }
    }
    await disconnect();
    // Las pantallas pueden preparar archivos: se deja el fixture como estaba
    // antes de arrancar la app del siguiente tema.
    try {
      git(getFixture().repo, ['reset', '-q']);
    } catch {
      /* sin fixture no hay nada que restaurar */
    }
  }

  stopDriver();
  cleanupFixtures();

  console.log('\n──────── regresión visual ────────');
  for (const r of results) {
    const mark = r.status === 'ok' || r.status === 'baseline' ? '✔' : '✖';
    console.log(`${mark} ${r.key.padEnd(30)} ${r.status.padEnd(12)} ${r.detail}`);
  }

  if (args.update) {
    console.log(`\nbaselines escritos en ${baselineDir}`);
    const manifest = path.join(baselineDir, 'manifest.json');
    writeFileSync(
      manifest,
      `${JSON.stringify(
        {
          platform,
          generatedAt: new Date().toISOString(),
          pixelThreshold: PIXEL_THRESHOLD,
          maxDiffRatio: MAX_DIFF_RATIO,
          screens: results.filter((r) => r.status === 'baseline').map((r) => r.key)
        },
        null,
        2
      )}\n`
    );
    mkdirSync(baselineDir, { recursive: true });
    process.exit(0);
  }

  const failed = results.filter((r) => r.status !== 'ok');
  console.log(
    `\n${results.length - failed.length}/${results.length} pantallas iguales al baseline` +
      (failed.length ? ` · diffs en ${diffDir}` : '')
  );
  process.exit(failed.length === 0 ? 0 : 1);
}

main().catch(async (err) => {
  console.error('\n✖ la regresión visual no pudo ejecutarse:', err?.message ?? err);
  try {
    await disconnect();
  } catch {
    /* nada */
  }
  stopDriver();
  cleanupFixtures();
  process.exit(2);
});
