//! Pantallas capturadas por la regresión visual. Cada entrada sabe dejar la
//! interfaz en un estado reproducible a partir del fixture limpio.
//!
//! El armazón nuevo es: riel de ramas (izquierda) · vista central con control
//! segmentado (Cambios · Historial · Stashes · PR · Conflictos) · panel derecho.

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { showView } from '../lib/flows.mjs';
import { clickTestId, fill, press, selectFile, toggleStage } from '../lib/ui.mjs';

const b = () => getBrowser();

/** Espera a que no haya operaciones en curso ni esqueletos de carga. */
async function settled() {
  const deadline = Date.now() + 30_000;
  while (Date.now() < deadline) {
    const busy = await b().execute(() =>
      Array.from(document.querySelectorAll('[aria-busy="true"]')).map((el) => ({
        tag: el.tagName,
        cls: String(el.className ?? '').slice(0, 60),
        label: el.getAttribute('aria-label')
      }))
    );
    if (busy.length === 0) break;
    await b().pause(200);
  }
  const still = await b().execute(() =>
    Array.from(document.querySelectorAll('[aria-busy="true"]')).map((el) => ({
      tag: el.tagName,
      cls: String(el.className ?? '').slice(0, 60),
      label: el.getAttribute('aria-label')
    }))
  );
  if (still.length > 0) throw new Error(`interfaz ocupada: ${JSON.stringify(still)}`);
  // Un esqueleto puede quedarse si la app está esperando contenido; no impide
  // capturar, así que se espera un poco y se sigue.
  for (let i = 0; i < 10; i += 1) {
    if (!(await b().execute(() => !!document.querySelector('.skeleton')))) break;
    await b().pause(300);
  }
  await b().pause(250);
}

/** Cierra cualquier capa modal que quede abierta (ajustes, paleta, remotos). */
async function closeOverlays() {
  await b().execute(() => {
    const closeSettings = document.querySelector('[data-testid="settings-close"]');
    if (closeSettings) {
      closeSettings.click();
      return;
    }
    const dialog = document.querySelector('[role="dialog"]');
    // La paleta escucha Escape en su propio contenedor.
    dialog?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  });
  await b().pause(150);
  if (await b().execute(() => !!document.querySelector('[role="dialog"]'))) {
    await b().keys(['Escape']);
    await b().pause(120);
  }
}

/** Activa (o crea) la pestaña en blanco con la pantalla de bienvenida. */
async function gotoWelcome() {
  await closeOverlays();
  const activated = await b().execute(() => {
    const tab = document.querySelector('[role="tab"][data-tab-kind="new"]');
    if (!tab) return false;
    tab.click();
    return true;
  });
  if (!activated) await clickTestId('tab-new');
  await waitInPage(() => !!document.querySelector('[data-testid="welcome"]'), [], { message: 'bienvenida' });
  await settled();
}

/** Vuelve a la pestaña del repositorio abierto. */
async function backToRepo() {
  await closeOverlays();
  await b().execute(() => {
    const tab = document.querySelector('[role="tab"][data-tab-kind="repo"]');
    tab?.click();
  });
  await waitInPage(() => !!document.querySelector('[data-testid="workspace"]'), [], {
    message: 'espacio de trabajo'
  });
  await settled();
}

export const screens = [
  {
    name: 'bienvenida',
    prepare: async () => {
      await gotoWelcome();
    }
  },
  {
    name: 'cambios',
    prepare: async () => {
      await backToRepo();
      await showView('changes');
      await settled();
    }
  },
  {
    name: 'diff',
    prepare: async () => {
      await selectFile('src/app.js');
      await waitInPage(() => (document.querySelector('[data-testid="diff-region"]')?.innerText ?? '').includes('export const version = 2'), [], {
        message: 'diff visible'
      });
      await settled();
    }
  },
  {
    name: 'historial',
    prepare: async () => {
      await showView('history');
      await waitInPage(() => /\d+/.test(document.querySelector('[aria-live="polite"]')?.textContent ?? ''), [], {
        message: 'historial cargado'
      });
      await settled();
    }
  },
  {
    // El riel sustituye a la antigua pestaña «Ramas»: rama actual, árbol y
    // inspector de la rama seleccionada.
    name: 'riel',
    prepare: async () => {
      await showView('changes');
      await clickTestId('rail-branch').catch(() => {});
      await b().pause(150);
      await settled();
    }
  },
  {
    name: 'ajustes',
    prepare: async () => {
      await press(['Control', ',']);
      await waitInPage(() => !!document.querySelector('#settings-title'), [], { message: 'diálogo de ajustes' });
      await settled();
    }
  },
  {
    name: 'paleta',
    prepare: async () => {
      await closeOverlays();
      await press(['Control', 'k']);
      await waitInPage(() => !!document.querySelector('input[role="combobox"]'), [], { message: 'paleta de comandos' });
      await fill('input[role="combobox"]', 'rama');
      await b().pause(200);
      await settled();
    }
  },
  {
    name: 'commit-preparado',
    prepare: async () => {
      await closeOverlays();
      await showView('changes');
      // Todo al índice con la casilla de cada fila (sin depender de textos).
      for (const file of ['src/app.js', 'docs/guia.md', 'nuevo.txt']) await toggleStage(file);
      await b().pause(400);
      // Al preparar, el diff seleccionado deja de existir: se pide el de un archivo
      // ya preparado para que el panel derecho no se quede «cargando».
      await selectFile('preparado.txt');
      await waitInPage(
        () => (document.querySelector('[data-testid="diff-region"]')?.innerText ?? '').includes('ya está en el índice'),
        [],
        { message: 'diff del archivo preparado' }
      );
      await fill('input[id^="commit-summary-"]', 'feat: preparar cambios desde la interfaz');
      await b().pause(250);
      await settled();
      if (!(await b().execute(() => !!document.querySelector('[data-testid="commit-submit"]'))))
        throw new Error('no se ve la barra de commit');
    }
  }
];
