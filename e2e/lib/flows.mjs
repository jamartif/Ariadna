//! Pasos compartidos por varios flujos: abrir el panel, asegurar el repo abierto.

import { getFixture } from './state.mjs';
import { getBrowser, waitInPage } from './session.mjs';
import { click, clickSelector, count, hasText } from './ui.mjs';

export const PANEL_TABLIST = '[aria-label="Secciones del repositorio"]';

/** ¿Está abierto el espacio de trabajo de un repositorio? */
export async function workspaceOpen() {
  const b = getBrowser();
  return b.execute(
    (sel) => !!document.querySelector(sel) && document.querySelectorAll('[role="tab"][data-tab-id]').length > 0,
    PANEL_TABLIST
  );
}

/** Abre el repositorio del fixture si aún no está abierto (por ruta, como un usuario). */
export async function ensureRepoOpen() {
  if (await workspaceOpen()) return;
  const { repo } = getFixture();
  const b = getBrowser();

  // Si hay una pestaña «Nueva pestaña», actívala; si no, créala.
  try {
    await clickSelector('button.new', { timeout: 5000 });
  } catch {
    /* ya hay una pestaña en blanco enfocada */
  }

  await b.waitUntil(async () => (await b.$$('input[placeholder="/ruta/al/repositorio"]')).length > 0, {
    timeout: 20_000,
    timeoutMsg: 'no apareció el campo de ruta de la pantalla de bienvenida'
  });
  const input = await b.$('input[placeholder="/ruta/al/repositorio"]');
  await input.click();
  await input.setValue(repo);
  await click('Abrir');
  await waitInPage((sel) => !!document.querySelector(sel), [PANEL_TABLIST], { message: 'espacio de trabajo' });
}

/** Cambia de panel en la columna central (Cambios, Historial, Ramas…). */
export async function showPanel(name) {
  await ensureRepoOpen();
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute(
        (label, want) => {
          const list = document.querySelector(label);
          if (!list) return false;
          const tab = Array.from(list.querySelectorAll('[role="tab"]')).find((el) =>
            (el.textContent ?? '').replace(/\s+/g, ' ').trim().startsWith(want)
          );
          if (!tab) return false;
          if (tab.getAttribute('aria-selected') !== 'true') tab.click();
          return true;
        },
        PANEL_TABLIST,
        name
      ),
    { timeout: 20_000, interval: 150, timeoutMsg: `no se pudo abrir el panel «${name}»` }
  );
}

/** ¿La app sigue mostrando la bienvenida (ningún repo abierto)? */
export async function welcomeVisible() {
  return hasText('Bienvenida a Ariadna');
}

/**
 * Cierra las pestañas de repositorio que no sean del fixture (sesiones restauradas).
 * Deja siempre una pestaña en blanco activa. En Linux el estado ya viene aislado,
 * así que normalmente no hay nada que cerrar.
 */
export async function closeOtherTabs({ timeout = 15_000 } = {}) {
  const b = getBrowser();
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    const closed = await b.execute(() => {
      const repoTabs = Array.from(document.querySelectorAll('[role="tab"][data-tab-id]')).filter(
        (el) => (el.getAttribute('aria-label') ?? '').toLowerCase().includes('repositorios abiertos') === false
      );
      const target = repoTabs.find((el) => !/nueva pestaña$/i.test((el.getAttribute('aria-label') ?? '').trim()));
      if (!target) return false;
      const wrapper = target.closest('div');
      const btn = wrapper?.querySelector('button[aria-label^="Cerrar"]');
      if (!btn) return false;
      btn.click();
      return true;
    });
    if (!closed) return true;
    await b.pause(150);
  }
  return false;
}

/** Deja la app en una pestaña en blanco (pantalla de bienvenida). */
export async function normalizeStart() {
  const b = getBrowser();
  // La barra de título tarda un instante en montarse tras crear la sesión.
  await b.waitUntil(
    async () => b.execute(() => !!document.querySelector('[role="tablist"]')),
    { timeout: 30_000, interval: 200, timeoutMsg: 'la barra de pestañas no apareció' }
  );
  await closeOtherTabs();
  const active = await b.execute(() => {
    const el = document.querySelector('[role="tab"][aria-selected="true"]');
    return el ? (el.getAttribute('aria-label') ?? '') : '';
  });
  if (!/nueva pestaña/i.test(active)) {
    await clickSelector('button.new');
    await b.pause(150);
  }
  await waitInPage(() => document.body.innerText.includes('Bienvenida a Ariadna'), [], {
    message: 'pantalla de bienvenida al empezar'
  });
}

export { count };
