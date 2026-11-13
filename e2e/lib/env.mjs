//! Rutas, binarios y utilidades de entorno compartidas por E2E y regresión visual.

import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const E2E_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
export const REPO_ROOT = path.resolve(E2E_DIR, '..');

/** Plataforma en el sentido de los baselines: linux | darwin | win32. */
export function platformTag() {
  return process.env.E2E_PLATFORM ?? process.platform;
}

/** Directorio de artefactos (capturas, diffs, logs). Nunca se versiona. */
export const ARTIFACTS = path.join(E2E_DIR, 'artifacts');
export function artifactPath(...parts) {
  const p = path.join(ARTIFACTS, ...parts);
  mkdirSync(path.dirname(p), { recursive: true });
  return p;
}

/** Binario de la app. Por defecto el build de depuración de este árbol. */
export function appBinary() {
  const fromEnv = process.env.ARIADNA_BINARY;
  if (fromEnv) {
    if (!existsSync(fromEnv)) throw new Error(`ARIADNA_BINARY no existe: ${fromEnv}`);
    return fromEnv;
  }
  const candidates =
    process.platform === 'win32'
      ? ['ariadna-app.exe', 'ariadna-app']
      : ['ariadna-app', 'ariadna-app.exe'];
  for (const name of candidates) {
    const p = path.join(REPO_ROOT, 'src-tauri', 'target', 'debug', name);
    if (existsSync(p)) return p;
  }
  throw new Error(
    'No encuentro el binario de Ariadna en src-tauri/target/debug/. ' +
      'Compílalo con `cargo build -p ariadna-app` o define ARIADNA_BINARY=/ruta/al/binario.'
  );
}

/** Binario de tauri-driver. */
export function tauriDriverBin() {
  if (process.env.TAURI_DRIVER_BIN) return process.env.TAURI_DRIVER_BIN;
  const local = path.join(os.homedir(), '.cargo', 'bin', process.platform === 'win32' ? 'tauri-driver.exe' : 'tauri-driver');
  return existsSync(local) ? local : 'tauri-driver';
}

/** Driver nativo que usa tauri-driver (WebKitWebDriver en Linux; en Windows/macOS lo detecta solo). */
export function nativeDriverPath() {
  if (process.env.TAURI_DRIVER_NATIVE) return process.env.TAURI_DRIVER_NATIVE;
  if (process.platform !== 'linux') return null;
  for (const p of ['/usr/bin/WebKitWebDriver', '/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1/WebKitWebDriver']) {
    if (existsSync(p)) return p;
  }
  return null;
}

/** Directorio temporal de la suite (fixtures). Se puede fijar con E2E_TMP. */
export function tmpRoot() {
  const dir = process.env.E2E_TMP ?? path.join(os.tmpdir(), 'ariadna-e2e');
  mkdirSync(dir, { recursive: true });
  return dir;
}

/**
 * Aísla el estado de la app (ajustes, recientes, sesión) en un directorio temporal
 * para que la suite no dependa de lo que el usuario tenga abierto. Solo Linux
 * respeta estas variables con Tauri; en macOS/Windows se limpian las pestañas
 * desde la propia interfaz (`flows.closeOtherTabs`).
 */
export function isolateAppState() {
  if (process.platform !== 'linux') return null;
  const dir = path.join(tmpRoot(), 'app-state');
  rmSync(dir, { recursive: true, force: true });
  for (const [key, suffix] of [
    ['XDG_CONFIG_HOME', 'config'],
    ['XDG_DATA_HOME', 'data'],
    ['XDG_CACHE_HOME', 'cache'],
    ['XDG_STATE_HOME', 'state']
  ]) {
    const p = path.join(dir, suffix);
    mkdirSync(p, { recursive: true });
    process.env[key] = p;
  }
  return dir;
}

/** Ejecuta git en un repositorio y devuelve stdout limpio. Falla con un mensaje útil. */
export function git(repo, args, extraEnv = {}) {
  try {
    return execFileSync('git', args, {
      cwd: repo,
      encoding: 'utf8',
      env: {
        ...process.env,
        GIT_TERMINAL_PROMPT: '0',
        GIT_CONFIG_NOSYSTEM: '1',
        GIT_AUTHOR_NAME: 'E2E Ariadna',
        GIT_AUTHOR_EMAIL: 'e2e@ariadna.test',
        GIT_COMMITTER_NAME: 'E2E Ariadna',
        GIT_COMMITTER_EMAIL: 'e2e@ariadna.test',
        ...extraEnv
      },
      stdio: ['ignore', 'pipe', 'pipe']
    }).trim();
  } catch (err) {
    const stderr = err.stderr?.toString?.() ?? '';
    throw new Error(`git ${args.join(' ')} falló en ${repo}: ${stderr || err.message}`);
  }
}

/**
 * Entorno que necesita la WebView de WebKitGTK para arrancar sin GPU en Xvfb/CI.
 * Se aplica al proceso tauri-driver y lo hereda la app.
 */
export function webkitEnv() {
  if (process.platform !== 'linux') return {};
  return {
    WEBKIT_DISABLE_COMPOSITING_MODE: '1',
    WEBKIT_DISABLE_DMABUF_RENDERER: '1',
    LIBGL_ALWAYS_SOFTWARE: '1',
    GDK_BACKEND: 'x11',
    ...(process.env.DISPLAY ? {} : {})
  };
}
