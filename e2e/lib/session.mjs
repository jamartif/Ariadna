//! Arranque de tauri-driver + sesión WebDriverIO y utilidades de espera.

import { spawn } from 'node:child_process';
import { createWriteStream, mkdirSync } from 'node:fs';
import net from 'node:net';
import path from 'node:path';
import { remote } from 'webdriverio';

import { ARTIFACTS, E2E_LOCALE, appBinary, artifactPath, nativeDriverPath, tauriDriverBin, webkitEnv } from './env.mjs';

const HOST = process.env.E2E_HOST ?? '127.0.0.1';
const PORT = Number(process.env.E2E_PORT ?? 4444);
const DRIVER_START_TIMEOUT = Number(process.env.E2E_DRIVER_TIMEOUT ?? 45_000);

let browser = null;
let driverProc = null;

/** Sesión activa. Falla claro si algo se usa fuera de una sesión. */
export function getBrowser() {
  if (!browser) throw new Error('No hay sesión WebDriver activa: ¿se saltó el arranque del driver?');
  return browser;
}

export function setBrowser(b) {
  browser = b;
}

function waitForPort(host, port, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  return new Promise((resolve, reject) => {
    const attempt = () => {
      const socket = net.connect({ host, port });
      socket.once('connect', () => {
        socket.destroy();
        resolve();
      });
      socket.once('error', () => {
        socket.destroy();
        if (Date.now() > deadline) reject(new Error(`tauri-driver no escucha en ${host}:${port}`));
        else setTimeout(attempt, 200);
      });
    };
    attempt();
  });
}

export async function startDriver() {
  mkdirSync(ARTIFACTS, { recursive: true });
  const log = createWriteStream(path.join(ARTIFACTS, 'tauri-driver.log'), { flags: 'w' });
  const args = ['--port', String(PORT)];
  const native = nativeDriverPath();
  if (native) args.push('--native-driver', native);

  driverProc = spawn(tauriDriverBin(), args, {
    stdio: ['ignore', 'pipe', 'pipe'],
    env: { ...process.env, ...webkitEnv() },
    windowsHide: true
  });
  driverProc.stdout.pipe(log);
  driverProc.stderr.pipe(log);

  let exited = null;
  driverProc.once('exit', (code, signal) => {
    exited = `tauri-driver terminó (code=${code}, signal=${signal})`;
  });

  try {
    await waitForPort(HOST, PORT, DRIVER_START_TIMEOUT);
  } catch (err) {
    throw new Error(`${err.message}${exited ? ` · ${exited}` : ''} · ver artifacts/tauri-driver.log`);
  }
  if (exited) throw new Error(exited);
  return driverProc;
}

export function stopDriver() {
  if (driverProc && driverProc.exitCode === null) {
    driverProc.kill('SIGTERM');
    driverProc = null;
  }
}

/**
 * Crea la sesión WebDriver sobre el binario indicado. tauri-driver exige la
 * capacidad `tauri:options.application`; el resto son extras de WebdriverIO.
 */
export async function connect({ port = PORT, hostname = HOST, attempts = 3 } = {}) {
  const application = appBinary();
  let lastErr;
  for (let i = 1; i <= attempts; i += 1) {
    try {
      const b = await remote({
        hostname,
        port,
        path: '/',
        logLevel: process.env.E2E_WDIO_LOG ?? 'error',
        connectionRetryCount: 1,
        connectionRetryTimeout: 120_000,
        capabilities: {
          browserName: 'wry',
          'tauri:options': { application },
          'wdio:enforceWebDriverClassic': true
        }
      });
      browser = b;
      try {
        await b.setTimeouts({ implicit: 0, pageLoad: 60_000, script: 30_000 });
      } catch {
        /* algunos drivers no aceptan setTimeouts; los helpers esperan igual */
      }
      return b;
    } catch (err) {
      // WebKitWebDriver puede tardar en levantar su socket: se reintenta.
      lastErr = err;
      if (i < attempts) await new Promise((r) => setTimeout(r, 2000));
    }
  }
  throw lastErr;
}

export async function disconnect() {
  if (!browser) return;
  try {
    await browser.deleteSession();
  } catch {
    /* la app pudo cerrarse antes */
  }
  browser = null;
}

/**
 * Comprueba que la app sirve la interfaz empaquetada. Un `cargo build` sin
 * `--features tauri/custom-protocol` deja la app apuntando a `devUrl`
 * (http://localhost:5173): las pruebas pasarían contra el servidor de Vite y no
 * contra el binario, que es justo lo que no queremos.
 */
export async function assertPackaged() {
  const b = getBrowser();
  const url = await b.getUrl().catch(() => '');
  if (/localhost:5173|127\.0\.0\.1:5173/.test(url)) {
    throw new Error(
      `El binario carga ${url}: está compilado en modo desarrollo. ` +
        'Recompílalo con `cargo build -p ariadna-app --features tauri/custom-protocol` ' +
        '(y `npm run build` en ui/ antes) para que use ui/dist.'
    );
  }
  return url;
}

/** Guarda una captura en artifacts/screens/<name>.png. Nunca rompe la prueba. */
export async function screenshot(name) {
  if (!browser) return null;
  const safe = name.replace(/[^\w.-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 90);
  const file = artifactPath('screens', `${safe}.png`);
  try {
    await browser.saveScreenshot(file);
    return file;
  } catch {
    return null;
  }
}

/**
 * Espera a que una función evaluada en la página devuelva algo verdadero.
 * `fn` se serializa: no puede cerrar sobre variables del test; pásalas en `args`.
 */
export async function waitInPage(fn, args = [], { timeout = 15_000, message = 'condición no cumplida' } = {}) {
  const b = getBrowser();
  try {
    await b.waitUntil(async () => Boolean(await b.execute(fn, ...args)), {
      timeout,
      interval: 150,
      timeoutMsg: message
    });
  } catch (err) {
    const shot = await screenshot(`fallo-${message}`);
    const html = await b.execute(() => document.body?.innerText?.slice(0, 1200) ?? '').catch(() => '');
    throw new Error(`${err.message}${shot ? ` · captura: ${shot}` : ''}\n--- texto visible ---\n${html}`);
  }
}

/** Espera a que el selector exista en el DOM. */
export async function waitForSelector(selector, { timeout = 15_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(async () => (await b.$$(selector)).length > 0, {
    timeout,
    interval: 150,
    timeoutMsg: `no apareció el selector ${selector}`
  });
}

/**
 * Espera a que la interfaz esté en el idioma de las pruebas. El idioma guardado en
 * `settings.json` (Rust) se aplica al montar y deja `<html lang>`; hasta entonces
 * la ventana puede estar en el idioma del sistema operativo.
 */
export async function waitLocale(locale = E2E_LOCALE, { timeout = 30_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute((want) => (document.documentElement.lang ?? '').toLowerCase().startsWith(want), locale.toLowerCase()),
    { timeout, interval: 150, timeoutMsg: `la interfaz no se puso en «${locale}»` }
  );
}
