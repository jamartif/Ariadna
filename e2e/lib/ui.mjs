//! Ayudas para manejar la interfaz desde WebDriver.
//!
//! Regla: se busca por **rol**, `data-testid` o atributos estables de datos
//! (`data-tab-id`, `data-brail-name`, `data-section`). El texto visible solo se
//! usa en mensajes de error y para contenido que no se traduce (asuntos de
//! commit, nombres de rama, rutas de archivo).

import { getBrowser } from './session.mjs';

const norm = (s) => (s ?? '').replace(/\s+/g, ' ').trim();

/** Devuelve el texto visible del documento (útil en mensajes de error). */
export async function pageText() {
  const b = getBrowser();
  return b.execute(() => document.body?.innerText ?? '');
}

/** Nº de elementos que cumplen el selector CSS. */
export async function count(selector) {
  const b = getBrowser();
  return (await b.$$(selector)).length;
}

/** ¿Hay algún elemento cuyo aria-label o texto contenga `needle`? */
export async function hasText(needle, { selector = 'button,[role="tab"],[role="option"],h1,h2,h3,span,div,p,li,label' } = {}) {
  const b = getBrowser();
  return b.execute(
    (sel, want) => {
      const n = (s) => (s ?? '').replace(/\s+/g, ' ').trim();
      return Array.from(document.querySelectorAll(sel)).some(
        (el) => n(el.getAttribute('aria-label')).includes(want) || n(el.textContent).includes(want)
      );
    },
    selector,
    needle
  );
}

/** ¿Existe un elemento con exactamente ese `aria-label`? */
export async function hasLabel(label) {
  const b = getBrowser();
  return b.execute((want) => {
    const n = (s) => (s ?? '').replace(/\s+/g, ' ').trim();
    return Array.from(document.querySelectorAll('[aria-label]')).some((el) => n(el.getAttribute('aria-label')) === want);
  }, label);
}

/** ¿Existe el elemento con ese `data-testid`? */
export async function hasTestId(id) {
  const b = getBrowser();
  return b.execute((want) => !!document.querySelector(`[data-testid="${want}"]`), id);
}

/**
 * Pulsa el elemento que mejor coincide con `matcher` (aria-label o texto).
 * Los elementos interactivos pesan menos que sus contenedores: pulsar el `div`
 * que envuelve al botón no sirve de nada (fue un fallo real en el primer intento).
 * Para controles cuyo texto se traduce, preferir `clickTestId`/`clickSelector`.
 */
export async function click(matcher, { timeout = 15_000, within = null } = {}) {
  const b = getBrowser();
  let clicked = false;
  try {
    await b.waitUntil(
      async () => {
        clicked = await b.execute(
          (want, scope) => {
            const n = (s) => (s ?? '').replace(/\s+/g, ' ').trim();
            const rank = (el) => {
              if (el.tagName === 'BUTTON') return 0;
              if (el.tagName === 'A') return 1;
              if (el.tagName === 'INPUT') return 2;
              if (el.tagName === 'LABEL') return 3;
              if (el.getAttribute('role')) return 4;
              if (el.tagName === 'LI') return 5;
              if (el.getAttribute('role') === 'row') return 6;
              return 9;
            };
            const visible = (el) => {
              const r = el.getBoundingClientRect();
              const st = getComputedStyle(el);
              return r.width > 0 && r.height > 0 && st.visibility !== 'hidden' && st.display !== 'none' && !el.disabled;
            };
            const root = scope ? document.querySelector(scope) : document;
            if (!root) return false;
            const candidates = Array.from(
              root.querySelectorAll(
                'button,a,input,label,li,[role="tab"],[role="option"],[role="radio"],[role="menuitem"],[role="treeitem"],[role="button"],[role="row"]'
              )
            ).filter(visible);

            const score = (el) => {
              const label = n(el.getAttribute('aria-label'));
              const text = n(el.textContent);
              const q =
                label === want ? 0 : text === want ? 1 : label.includes(want) ? 2 : text.includes(want) ? 3 : -1;
              return q < 0 ? -1 : rank(el) * 10 + q;
            };

            const best = candidates
              .map((el) => ({ el, s: score(el) }))
              .filter((c) => c.s >= 0)
              .sort((a, c) => a.s - c.s)[0]?.el;
            if (!best) return false;
            best.scrollIntoView({ block: 'center', inline: 'nearest' });
            best.click();
            return true;
          },
          matcher,
          within
        );
        return clicked;
      },
      { timeout, interval: 150, timeoutMsg: `no se pudo pulsar «${matcher}»` }
    );
  } catch (err) {
    const text = await pageText().catch(() => '');
    throw new Error(`${err.message}\n--- texto visible ---\n${text.slice(0, 1500)}`);
  }
  return clicked;
}

/**
 * Escribe en un campo disparando los eventos reales (`input`/`change`) que usa
 * Svelte. No usa el clic del driver: con tostadas o capas flotantes encima,
 * WebKitWebDriver aborta con «element click intercepted».
 */
export async function fill(selector, text, { timeout = 15_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute(
        (sel, value) => {
          const el = document.querySelector(sel);
          if (!el) return false;
          el.focus();
          el.value = value;
          el.dispatchEvent(new Event('input', { bubbles: true }));
          el.dispatchEvent(new Event('change', { bubbles: true }));
          return el.value === value;
        },
        selector,
        text
      ),
    { timeout, interval: 150, timeoutMsg: `no se pudo escribir en ${selector}` }
  );
}

/** Pulsa Intro en el elemento enfocado. */
export async function pressEnter() {
  const b = getBrowser();
  await b.keys(['Enter']);
}

/** Pulsa una combinación de teclas en la ventana (`['Control', 'b']`). */
export async function press(keys) {
  const b = getBrowser();
  await b.keys(keys);
}

/** Nº de pestañas de repositorio en la barra de título (no cuenta los paneles internos). */
export async function tabCount() {
  const b = getBrowser();
  return b.execute(() => document.querySelectorAll('[role="tablist"] [role="tab"][data-tab-id]').length);
}

/**
 * Pestañas de la barra de título. Su nombre accesible sale del contenido visible
 * (repo, rama, contador) y del `title`: ya no hay `aria-label` (rompía
 * «Label in Name»), así que el texto y el `title` son la fuente.
 * @returns {Promise<{id: string, kind: string, title: string, text: string, selected: boolean}[]>}
 */
export async function tabs() {
  const b = getBrowser();
  return b.execute(() =>
    Array.from(document.querySelectorAll('[role="tab"][data-tab-id]')).map((el) => ({
      id: el.dataset.tabId ?? '',
      kind: el.dataset.tabKind ?? '',
      title: el.getAttribute('title') ?? '',
      text: (el.textContent ?? '').replace(/\s+/g, ' ').trim(),
      selected: el.getAttribute('aria-selected') === 'true'
    }))
  );
}

/**
 * Cierra una pestaña por su vía real. El botón de cerrar queda fuera del árbol de
 * accesibilidad (un `tablist` solo admite pestañas), así que el teclado es la vía
 * documentada: foco en la pestaña + `Supr`. Si el driver no entrega la tecla se
 * prueban `Ctrl+W` (atajo de la app) y, como último recurso, ese mismo botón.
 * @returns {Promise<'supr'|'ctrl+w'|'botón'>} la vía que cerró la pestaña.
 */
export async function closeTab(id, { timeout = 15_000 } = {}) {
  const b = getBrowser();
  const exists = () => b.execute((tid) => !!document.querySelector(`[role="tab"][data-tab-id="${tid}"]`), id);
  if (!(await exists())) throw new Error(`no existe la pestaña ${id}`);

  const gone = (ms) =>
    b
      .waitUntil(async () => !(await exists()), { timeout: ms, interval: 100 })
      .then(() => true)
      .catch(() => false);

  const active = await b.execute(
    (tid) => document.querySelector(`[role="tab"][data-tab-id="${tid}"]`)?.getAttribute('aria-selected') === 'true',
    id
  );
  const focused = await b.execute((tid) => {
    const el = document.querySelector(`[role="tab"][data-tab-id="${tid}"]`);
    if (!el) return false;
    el.focus();
    return document.activeElement === el;
  }, id);

  if (focused) {
    await b.keys(['Delete']);
    if (await gone(2000)) return 'supr';
    if (active) {
      await b.keys(['Control', 'w']);
      if (await gone(2000)) return 'ctrl+w';
    }
  }
  await b.execute((tid) => {
    const tab = document.querySelector(`[role="tab"][data-tab-id="${tid}"]`);
    tab?.parentElement?.querySelector('[data-testid="tab-close"]')?.click();
  }, id);
  await b.waitUntil(async () => !(await exists()), {
    timeout,
    interval: 150,
    timeoutMsg: `la pestaña ${id} no se cerró`
  });
  return 'botón';
}

/**
 * Pulsa un selector CSS directamente desde la página, sin pasar por el motor
 * de clics del driver (que falla con elementos tapados o sin caja estable).
 */
export async function clickSelector(selector, { timeout = 15_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute((sel) => {
        const el = document.querySelector(sel);
        if (!el) return false;
        el.click();
        return true;
      }, selector),
    { timeout, interval: 150, timeoutMsg: `no se pudo pulsar ${selector}` }
  );
}

/** Pulsa el elemento con ese `data-testid` (dentro de `within`, si se indica). */
export async function clickTestId(id, { timeout = 15_000, within = null } = {}) {
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute(
        (want, scope) => {
          const root = scope ? document.querySelector(scope) : document;
          const el = root?.querySelector(`[data-testid="${want}"]`);
          if (!el) return false;
          el.scrollIntoView({ block: 'nearest', inline: 'nearest' });
          el.click();
          return true;
        },
        id,
        within
      ),
    { timeout, interval: 150, timeoutMsg: `no se pudo pulsar [data-testid="${id}"]` }
  );
}

/** Espera a que aparezca el elemento con ese `data-testid`. */
export async function waitTestId(id, { timeout = 20_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(
    async () => b.execute((want) => !!document.querySelector(`[data-testid="${want}"]`), id),
    { timeout, interval: 150, timeoutMsg: `no apareció [data-testid="${id}"]` }
  );
}

/**
 * Selecciona la fila de un archivo en el panel de cambios. Se busca la celda con
 * `title` igual a la ruta, así funciona en vista de lista y de árbol y no se
 * confunde con los botones de preparar/descartar (que llevan la ruta en su etiqueta).
 */
export async function selectFile(path, { timeout = 15_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute((p) => {
        const cell = Array.from(document.querySelectorAll('[role="row"] [title]')).find(
          (el) => (el.getAttribute('title') ?? '') === p
        );
        const row = cell?.closest('[role="row"]');
        if (!row) return false;
        row.scrollIntoView({ block: 'center' });
        row.click();
        return true;
      }, path),
    { timeout, interval: 150, timeoutMsg: `no se pudo seleccionar el archivo ${path}` }
  );
}

/**
 * Prepara o quita del índice el archivo `path` con la casilla de su fila: la
 * casilla no se traduce (su `aria-label` sí).
 */
export async function toggleStage(path, { timeout = 15_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute((p) => {
        const cell = Array.from(document.querySelectorAll('[role="row"] [title]')).find(
          (el) => (el.getAttribute('title') ?? '') === p
        );
        const row = cell?.closest('[role="row"]');
        const box = row?.querySelector('input[type="checkbox"]');
        if (!box) return false;
        box.click();
        return true;
      }, path),
    { timeout, interval: 150, timeoutMsg: `no se pudo preparar ${path}` }
  );
}

/** Estado de la casilla de preparado de un archivo (`null` si no está en la lista). */
export async function fileStaged(path) {
  const b = getBrowser();
  return b.execute((p) => {
    const row = Array.from(document.querySelectorAll('[role="row"]')).find((r) =>
      Array.from(r.querySelectorAll('[title]')).some((c) => c.getAttribute('title') === p)
    );
    if (!row) return null;
    const box = row.querySelector('input[type="checkbox"]');
    return box ? box.checked : false;
  }, path);
}

/**
 * Sección del panel de cambios en la que está un archivo (`staged`, `unstaged`,
 * `untracked`, `conflicts`, `ignored`) o `null` si no está en la lista. Se recorre
 * la rejilla en orden: la cabecera de sección lleva `data-section`.
 */
export async function fileSection(path) {
  const b = getBrowser();
  return b.execute((p) => {
    let section = null;
    for (const row of Array.from(document.querySelectorAll('[role="row"]'))) {
      const own = row.getAttribute('data-section');
      if (own) section = own;
      const has = Array.from(row.querySelectorAll('[title]')).some((c) => c.getAttribute('title') === p);
      if (has) return section;
    }
    return null;
  }, path);
}

/** Espera a que el diálogo de ajustes termine de guardar (desaparece «Guardar»). */
export async function waitSettingsSaved({ timeout = 20_000 } = {}) {
  const b = getBrowser();
  await b.waitUntil(
    async () =>
      b.execute(() => {
        const dlg = document.querySelector('[role="dialog"]');
        if (!dlg) return false;
        return !dlg.querySelector('[data-testid="settings-save"]');
      }),
    { timeout, interval: 200, timeoutMsg: 'los ajustes no terminaron de guardarse' }
  );
}

/** Texto del contador del historial («100 commits cargados»). */
export async function historyCountText() {
  const b = getBrowser();
  return b.execute(() => {
    const el = document.querySelector('[aria-live="polite"]');
    return el ? el.textContent.replace(/\s+/g, ' ').trim() : '';
  });
}

/** Desplaza la lista del historial hasta el final para forzar `loadMore`. */
export async function scrollHistoryToBottom() {
  const b = getBrowser();
  return b.execute(() => {
    const list = document.querySelector('[role="listbox"]');
    if (!list) return false;
    let scroller = list;
    while (scroller && scroller.scrollHeight <= scroller.clientHeight && scroller !== document.body) {
      scroller = scroller.parentElement;
    }
    if (!scroller) return false;
    scroller.scrollTop = scroller.scrollHeight;
    scroller.dispatchEvent(new Event('scroll', { bubbles: true }));
    return true;
  });
}

/** Tema efectivo aplicado al documento. */
export async function currentTheme() {
  const b = getBrowser();
  return b.execute(() => document.documentElement.getAttribute('data-theme'));
}

/** Idioma activo según `<html lang>` (la app lo fija al aplicar el guardado). */
export async function currentLang() {
  const b = getBrowser();
  return b.execute(() => document.documentElement.lang ?? '');
}

export { norm };
