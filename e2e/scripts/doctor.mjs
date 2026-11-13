#!/usr/bin/env node
//! Comprueba que el entorno puede ejecutar la suite: versiones, drivers y binario.

import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import os from 'node:os';

import { appBinary, nativeDriverPath, tauriDriverBin } from '../lib/env.mjs';

function run(cmd, args = ['--version']) {
  try {
    return execFileSync(cmd, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim().split('\n')[0];
  } catch (err) {
    return `✖ ${err.message.split('\n')[0]}`;
  }
}

const checks = [];
const add = (name, ok, detail) => checks.push({ name, ok, detail });

add('Node ≥ 22.12', Number(process.versions.node.split('.')[0]) >= 22, process.version);
add('git', !run('git').startsWith('✖'), run('git'));
add('Sistema', true, `${process.platform} ${process.arch} · ${os.release()}`);

const driver = tauriDriverBin();
const driverOk = driver !== 'tauri-driver' ? existsSync(driver) : !run('tauri-driver', ['--help']).startsWith('✖');
add('tauri-driver', driverOk, driverOk ? driver : 'instala con `cargo install tauri-driver --locked`');

if (process.platform === 'linux') {
  const native = nativeDriverPath();
  add('WebKitWebDriver', !!native, native ?? 'falta: apt-get install webkit2gtk-driver (o webkitgtk-webdriver)');
  add('DISPLAY o Xvfb', !!process.env.DISPLAY, process.env.DISPLAY ?? 'usa `xvfb-run -a -s "-screen 0 1440x900x24"`');
}

try {
  const bin = appBinary();
  add('binario de la app', true, bin);
} catch (err) {
  add('binario de la app', false, err.message);
}

for (const c of checks) {
  console.log(`${c.ok ? '✔' : '✖'} ${c.name.padEnd(18)} ${c.detail}`);
}

const failed = checks.filter((c) => !c.ok).length;
console.log(failed === 0 ? '\nTodo listo.' : `\n${failed} comprobación(es) pendiente(s).`);
process.exit(failed === 0 ? 0 : 1);
