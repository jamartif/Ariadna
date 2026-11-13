//! Pantallas capturadas por la regresión visual. Cada entrada sabe dejar la
//! interfaz en un estado reproducible a partir del fixture limpio.

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { showPanel } from '../lib/flows.mjs';
import { click, fill, hasText, scrollHistoryToBottom, selectFile } from '../lib/ui.mjs';

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

async function closeOverlays() {
  await b().execute(() => {
    const dialog = document.querySelector('[role="dialog"]');
    if (!dialog) return;
    const closeSettings = document.querySelector('[aria-label="Cerrar ajustes"]');
    if (closeSettings) {
      closeSettings.click();
      return;
    }
    // La paleta escucha Escape en su propio contenedor.
    dialog.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
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
  const hasBlank = await b().execute(() => {
    const tab = Array.from(document.querySelectorAll('[role="tab"][data-tab-id]')).find((el) =>
      /nueva pestaña/i.test((el.getAttribute('aria-label') ?? '').trim())
    );
    if (!tab) return false;
    tab.click();
    return true;
  });
  if (!hasBlank) await (await b().$('button.new')).click();
  await waitInPage(() => document.body.innerText.includes('Bienvenida a Ariadna'), [], {
    message: 'bienvenida'
  });
  await settled();
}

async function backToRepo() {
  await b().execute(() => {
    const tab = Array.from(document.querySelectorAll('[role="tab"][data-tab-id]')).find(
      (el) => !(el.getAttribute('aria-label') ?? '').toLowerCase().includes('nueva pestaña')
    );
    tab?.click();
  });
  await waitInPage(() => !!document.querySelector('[aria-label="Secciones del repositorio"]'), [], {
    message: 'espacio de trabajo'
  });
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
      await showPanel('Cambios');
      await settled();
    }
  },
  {
    name: 'diff',
    prepare: async () => {
      await selectFile('src/app.js');
      await waitInPage(() => document.body.innerText.includes('export const version = 2'), [], {
        message: 'diff visible'
      });
      await settled();
    }
  },
  {
    name: 'historial',
    prepare: async () => {
      await showPanel('Historial');
      await waitInPage(() => /\d+ commits? cargados/.test(document.body.innerText), [], {
        message: 'historial cargado'
      });
      await scrollHistoryToBottom();
      await b().pause(300);
      await settled();
    }
  },
  {
    name: 'ramas',
    prepare: async () => {
      await showPanel('Ramas');
      await waitInPage(() => document.body.innerText.includes('Nueva rama'), [], { message: 'panel de ramas' });
      await settled();
    }
  },
  {
    name: 'ajustes',
    prepare: async () => {
      await click('Ajustes');
      await waitInPage(() => !!document.querySelector('[role="dialog"]'), [], { message: 'diálogo de ajustes' });
      await settled();
    }
  },
  {
    name: 'paleta',
    prepare: async () => {
      await b().execute(() => {
        const dlg = document.querySelector('[role="dialog"]');
        if (dlg) {
          const close = dlg.querySelector('[aria-label="Cerrar ajustes"]');
          close?.click();
        }
      });
      await b().pause(150);
      await click('Abrir paleta de comandos');
      await waitInPage(() => !!document.querySelector('[role="dialog"]'), [], { message: 'paleta de comandos' });
      await fill('input[placeholder="Escribe un comando…"]', 'rama');
      await b().pause(200);
      await settled();
    }
  },
  {
    name: 'commit-preparado',
    prepare: async () => {
      await closeOverlays();
      await showPanel('Cambios');
      // Sin selección no hay diff a medio cargar tapando la captura.
      await b().execute(() => {
        const btn = Array.from(document.querySelectorAll('button')).find(
          (el) => (el.getAttribute('aria-label') ?? '') === 'Borrar selección'
        );
        btn?.click();
      });
      await b().pause(200);
      // «Preparar todo» aparece en cada sección (sin preparar / sin seguimiento).
      for (let i = 0; i < 3; i += 1) {
        const clicked = await b().execute(() => {
          const btn = Array.from(document.querySelectorAll('button')).find(
            (el) => (el.textContent ?? '').trim() === 'Preparar todo'
          );
          if (!btn) return false;
          btn.click();
          return true;
        });
        if (!clicked) break;
        await b().pause(400);
      }
      // Al preparar, el diff seleccionado deja de existir: se pide el de un archivo
      // ya preparado para que el panel derecho no se quede «cargando».
      await selectFile('preparado.txt');
      await waitInPage(
        () => {
          const pane = document.querySelector('[aria-label="Diferencias"]');
          return (pane?.innerText ?? '').includes('ya está en el índice');
        },
        [],
        { message: 'diff del archivo preparado' }
      );
      await fill('input[placeholder="Asunto del commit"]', 'feat: preparar cambios desde la interfaz');
      await b().pause(250);
      await settled();
      if (!(await hasText('Crear commit'))) throw new Error('no se ve la barra de commit');
    }
  }
];
