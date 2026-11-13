//! Pasos compartidos por los flujos: abrir el repositorio, moverse por las vistas
//! del centro y manejar el riel de ramas (que sustituyó a la pestaña «Ramas»).
//!
//! Todo se localiza por `data-testid` o atributos de datos: los nombres de rama,
//! rutas y asuntos vienen del fixture y no se traducen.

import { getFixture } from './state.mjs';
import { getBrowser, waitInPage } from './session.mjs';
import { clickSelector, clickTestId, closeTab, count, fill, hasTestId, tabCount, tabs } from './ui.mjs';

/** Vistas centrales del control segmentado (Cambios · Historial · Stashes · PR · Conflictos). */
export const VIEW_IDS = ['changes', 'history', 'stashes', 'pullRequests', 'conflicts'];
export const WORKSPACE = '[data-testid="workspace"]';
export const RAIL = '[data-testid="rail"]';

/** ¿Está abierto el espacio de trabajo de un repositorio? */
export async function workspaceOpen() {
  const b = getBrowser();
  return b.execute(() => !!document.querySelector('[data-testid="workspace"]') && !!document.querySelector('[data-tab-id]'));
}

/** Abre el repositorio del fixture si aún no está abierto (por ruta, como un usuario). */
export async function ensureRepoOpen() {
  if (await workspaceOpen()) return;
  const { repo } = getFixture();
  const b = getBrowser();

  // Si hay una pestaña «Nueva pestaña» inactiva, actívala; si no, créala.
  const list = await tabs();
  const blank = list.find((t) => t.kind === 'new');
  if (blank) {
    if (!blank.selected) await clickTab(blank.id);
  } else {
    try {
      await clickSelector('[data-testid="tab-new"]', { timeout: 5000 });
    } catch {
      /* ya hay una pestaña en blanco enfocada */
    }
  }

  await b.waitUntil(
    async () => (await b.$$('input[id^="manual-path-"]')).length > 0,
    { timeout: 20_000, interval: 150, timeoutMsg: 'no apareció el campo de ruta de la pantalla de bienvenida' }
  );
  await fill('input[id^="manual-path-"]', repo);
  await b.execute(() => {
    const input = document.querySelector('input[id^="manual-path-"]');
    input?.closest('form')?.requestSubmit();
  });
  await waitInPage((sel) => !!document.querySelector(sel), [WORKSPACE], { message: 'espacio de trabajo' });
}

/** Activa (sin cerrar) la pestaña con ese id. */
export async function clickTab(id) {
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute((tid) => {
        const el = document.querySelector(`[role="tab"][data-tab-id="${tid}"]`);
        if (!el) return false;
        el.click();
        return true;
      }, id),
    { timeout: 15_000, interval: 150, timeoutMsg: `no se pudo activar la pestaña ${id}` }
  );
  await b.waitUntil(
    async () =>
      b.execute(
        (tid) => document.querySelector(`[role="tab"][data-tab-id="${tid}"]`)?.getAttribute('aria-selected') === 'true',
        id
      ),
    { timeout: 15_000, interval: 150, timeoutMsg: `la pestaña ${id} no quedó activa` }
  );
}

/** Cambia la vista central (Cambios/Historial/Stashes/PR/Conflictos) desde el control segmentado. */
export async function showView(id) {
  await ensureRepoOpen();
  const b = getBrowser();
  if (!VIEW_IDS.includes(id)) throw new Error(`vista desconocida: ${id}`);
  await b.waitUntil(
    async () =>
      b.execute((want) => {
        const tab = document.querySelector(`[data-testid="view-${want}"]`);
        if (!tab) return false;
        if (tab.getAttribute('aria-selected') !== 'true') tab.click();
        return true;
      }, id),
    { timeout: 20_000, interval: 150, timeoutMsg: `no se pudo abrir la vista «${id}»` }
  );
  await b.waitUntil(
    async () =>
      b.execute(
        (want) => document.querySelector(`[data-testid="view-${want}"]`)?.getAttribute('aria-selected') === 'true',
        id
      ),
    { timeout: 20_000, interval: 150, timeoutMsg: `la vista «${id}» no quedó activa` }
  );
}

/** ¿La app sigue mostrando la bienvenida (ningún repo abierto)? */
export async function welcomeVisible() {
  return hasTestId('welcome');
}

/** Estado del riel: `true` si está contraído. */
export async function railCollapsed() {
  const b = getBrowser();
  return b.execute(() => document.querySelector('[data-testid="rail"]')?.getAttribute('data-collapsed') === 'true');
}

/** Abre el riel si está contraído. La vía normal es el atajo `Ctrl+B`; si el driver
 *  no entrega la tecla, se pulsa el botón de expandir (mismo efecto). */
export async function ensureRailOpen({ timeout = 15_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(async () => b.execute(() => !!document.querySelector('[data-testid="rail"]')), {
    timeout,
    interval: 150,
    timeoutMsg: 'no apareció el riel de ramas'
  });
  if (!(await railCollapsed())) return;
  await b.keys(['Control', 'b']);
  const opened = await b
    .waitUntil(async () => !(await railCollapsed()), { timeout: 2000, interval: 100 })
    .then(() => true)
    .catch(() => false);
  if (!opened) await clickTestId('rail-expand');
  await b.waitUntil(async () => !(await railCollapsed()), {
    timeout,
    interval: 150,
    timeoutMsg: 'el riel de ramas no se abrió'
  });
}

/** Escribe en el filtro del riel (instantáneo, filtra el árbol de ramas). */
export async function filterBranches(text) {
  await ensureRailOpen();
  await fill('[data-testid="rail-filter"]', text);
}

/** Nombre de la rama actual según la tarjeta del riel. */
export async function currentBranchName() {
  const b = getBrowser();
  return b.execute(() => document.querySelector('[data-testid="rail-current-name"]')?.textContent?.trim() ?? '');
}

/** ¿El riel lista esa rama? */
export async function branchListed(name) {
  const b = getBrowser();
  return b.execute((want) => !!Array.from(document.querySelectorAll('[data-brail-name]')).find((el) => el.dataset.brailName === want), name);
}

/**
 * Selecciona una rama en el riel (un clic la selecciona; el inspector muestra sus
 * acciones y el panel derecho su detalle).
 */
export async function selectBranch(name, { timeout = 20_000 } = {}) {
  await ensureRailOpen();
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute((want) => {
        const el = Array.from(document.querySelectorAll('[data-brail-name]')).find((x) => x.dataset.brailName === want);
        if (!el) return false;
        el.scrollIntoView({ block: 'nearest' });
        el.click();
        return true;
      }, name),
    { timeout, interval: 150, timeoutMsg: `no se encontró la rama «${name}» en el riel` }
  );
  await b.waitUntil(
    async () => b.execute((want) => !!document.querySelector('[data-testid="rail-inspector"]'), name),
    { timeout, interval: 150, timeoutMsg: `la rama «${name}» no quedó seleccionada` }
  );
}

/**
 * Cambia a la rama indicada desde el inspector del riel. Si git se queja de
 * cambios sin confirmar, la app pide confirmación: se acepta «cambiar igualmente».
 */
export async function checkoutBranch(name, { confirmDirty = true, timeout = 30_000 } = {}) {
  await selectBranch(name);
  await clickTestId('rail-checkout');
  const b = getBrowser();
  const prompt = await b
    .waitUntil(async () => hasTestId('rail-checkout-prompt'), { timeout: 5000, interval: 150 })
    .then(() => true)
    .catch(() => false);
  if (prompt) {
    if (!confirmDirty) throw new Error(`git pidió confirmación para cambiar a «${name}»`);
    await clickTestId('rail-switch-anyway');
  }
  await b.waitUntil(async () => (await currentBranchName()) === name, {
    timeout,
    interval: 250,
    timeoutMsg: `la rama actual del riel no pasó a «${name}»`
  });
}

/** Crea una rama desde la cabecera del riel (nombre y, opcionalmente, punto de partida). */
export async function createBranch(name, { from = null, timeout = 30_000 } = {}) {
  await ensureRailOpen();
  await clickTestId('rail-new-branch');
  const b = getBrowser();
  await b.waitUntil(async () => (await b.$$('#rail-new-name')).length > 0, {
    timeout: 10_000,
    interval: 150,
    timeoutMsg: 'no apareció el formulario de nueva rama del riel'
  });
  await fill('#rail-new-name', name);
  if (from !== null) await fill('#rail-new-start', from);
  await clickTestId('rail-create-submit');
  await b.waitUntil(async () => branchListed(name), {
    timeout,
    interval: 250,
    timeoutMsg: `la rama «${name}» no aparece en el riel`
  });
}

/**
 * Deja la app con una única pestaña en blanco (pantalla de bienvenida). Cierra
 * las pestañas de repositorio que hayan quedado de una ejecución anterior.
 */
export async function normalizeStart() {
  const b = getBrowser();
  // La barra de título tarda un instante en montarse tras crear la sesión.
  await b.waitUntil(async () => b.execute(() => !!document.querySelector('[role="tablist"]')), {
    timeout: 30_000,
    interval: 200,
    timeoutMsg: 'la barra de pestañas no apareció'
  });
  for (const tab of (await tabs()).filter((t) => t.kind === 'repo')) await closeTab(tab.id);
  const list = await tabs();
  const blank = list.find((t) => t.kind === 'new');
  if (blank) await clickTab(blank.id);
  else await clickTestId('tab-new');
  await waitInPage((sel) => !!document.querySelector(sel), ['[data-testid="welcome"]'], {
    message: 'pantalla de bienvenida al empezar'
  });
}

export { count, tabCount };
