//! Flujo 10 · menú contextual de una rama del riel.
//!
//! Cubre las tres vías del componente (`BranchMenu.svelte`): apertura con **clic derecho**,
//! apertura con **teclado** (Mayús+F10 sobre la fila), acciones **deshabilitadas con motivo**
//! en la rama actual, ejecución real de «Copiar el nombre» (con el portapapeles interceptado:
//! ni permisos del sistema ni red) y «Ver el diff», y cierre con **Escape** devolviendo el
//! foco a la fila. El fixture no tiene remoto, así que «Abrir en el navegador» y «Crear pull
//! request» son justamente las acciones deshabilitadas por falta de remoto web.

import assert from 'node:assert/strict';

import { getBrowser } from '../lib/session.mjs';
import {
  branchMenuItem,
  branchMenuState,
  closeBranchMenuWithEscape,
  createBranch,
  currentBranchName,
  ensureRailOpen,
  ensureRepoOpen,
  focusedMenuItem,
  openBranchMenuByKeyboard,
  openBranchMenuByRightClick,
  railBranches,
  showView,
  trapClipboard,
  trappedClipboard
} from '../lib/flows.mjs';
import { clickTestId, hasTestId, waitTestId } from '../lib/ui.mjs';

/** Rama del flujo: se crea si no existe, para que el fichero funcione aislado (`--grep`). */
const WORK_BRANCH = 'feature/e2e-menu';

/** Rama distinta de la actual (el fixture no tiene remotos: todas las filas son locales). */
function otherLocalBranch(branches, current) {
  return branches.find((n) => n !== current) ?? null;
}

/** Deja el riel con al menos dos ramas locales y devuelve la actual y la otra. */
async function twoLocalBranches() {
  await ensureRepoOpen();
  await ensureRailOpen();
  if (!(await railBranches()).includes(WORK_BRANCH)) await createBranch(WORK_BRANCH);
  const branches = await railBranches();
  const current = await currentBranchName();
  const other = otherLocalBranch(branches, current) ?? otherLocalBranch(branches, branches[0]);
  return { current, other, branches };
}

describe('Flujo 10 · menú contextual de una rama', () => {
  it('abre con clic derecho, deshabilita lo que no aplica y cierra con Escape devolviendo el foco', async () => {
    await showView('changes');
    const { current, other, branches } = await twoLocalBranches();
    assert.ok(other, `el riel debería listar otra rama local además de «${current}»: ${branches.join(', ')}`);

    // --- Clic derecho en una rama que NO es la actual: las acciones de escritura están disponibles.
    await openBranchMenuByRightClick(other);
    const state = await branchMenuState();
    assert.equal(state.mode, 'actions', 'el menú debería abrirse en el paso de acciones');
    assert.ok(state.label.includes(other), `el nombre accesible debería citar la rama: ${state.label}`);
    assert.ok(state.items.length >= 10, `el menú debería tener todas las acciones: ${state.items.length}`);

    for (const id of ['checkout', 'from', 'merge', 'rebase', 'rename', 'delete', 'copy-name', 'copy-sha', 'diff']) {
      const item = state.items.find((i) => i.id === id);
      assert.ok(item, `debería existir la acción «${id}»`);
      assert.equal(item.disabled, false, `«${id}» debería estar disponible en una rama que no es la actual`);
      assert.equal(item.title, '', `«${id}» no debería llevar motivo de bloqueo`);
    }

    // Sin remoto en el fixture: los enlaces web quedan deshabilitados **con su motivo**.
    for (const id of ['open', 'pr']) {
      const item = await branchMenuItem(id);
      assert.equal(item.disabled, true, `«${id}» debería estar deshabilitada sin remoto`);
      assert.ok(item.title.length > 0, `«${id}» debería explicar el motivo en el title`);
    }

    // Escape cierra y devuelve el foco a la fila.
    await closeBranchMenuWithEscape(other);
    assert.equal(await branchMenuState(), null, 'el menú debería estar cerrado');

    // --- Misma acción por teclado: Mayús+F10 con la fila enfocada.
    await openBranchMenuByKeyboard(other);
    assert.ok(await branchMenuState(), 'el menú debería abrirse con Mayús+F10');
    assert.equal(await focusedMenuItem(), 'checkout', 'el foco debería empezar en la primera acción');

    await getBrowser().keys(['ArrowDown']);
    assert.equal(await focusedMenuItem(), 'from', '↓ debería mover el foco a la segunda acción');
    await getBrowser().keys(['End']);
    assert.equal(await focusedMenuItem(), 'pr', 'Fin debería llevar a la última acción');

    await closeBranchMenuWithEscape(other);

    // --- La rama actual: lo que no aplica queda deshabilitado con motivo.
    await openBranchMenuByRightClick(current);
    const currentState = await branchMenuState();
    for (const id of ['checkout', 'merge', 'rebase', 'delete', 'diff']) {
      const item = currentState.items.find((i) => i.id === id);
      assert.ok(item, `la rama actual debería tener «${id}» en el menú`);
      assert.equal(item.disabled, true, `«${id}» debería estar deshabilitado en la rama actual`);
      assert.ok(item.title.length > 0, `«${id}» debería explicar por qué no se puede usar`);
    }
    // Pulsar una acción deshabilitada no hace nada: el menú sigue abierto.
    await clickTestId(`branch-menu-checkout`, { timeout: 5000 });
    assert.ok(await branchMenuState(), 'una acción deshabilitada no debería ejecutarse ni cerrar el menú');
    await closeBranchMenuWithEscape(current);
  });

  it('ejecuta «Copiar el nombre» y «Ver el diff» y pinta el diff en el panel derecho', async () => {
    const { current, other } = await twoLocalBranches();
    assert.ok(other, 'hace falta otra rama local para el diff');

    // --- «Copiar el nombre»: portapapeles interceptado, sin permisos ni sistema.
    await trapClipboard();
    await openBranchMenuByRightClick(other);
    const copy = await branchMenuItem('copy-name');
    assert.equal(copy.disabled, false, '«Copiar el nombre» debería estar disponible');
    await clickTestId('branch-menu-copy-name');
    await getBrowser().waitUntil(async () => (await branchMenuState()) === null, {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: 'el menú no se cerró al ejecutar «Copiar el nombre»'
    });
    const copied = await trappedClipboard();
    assert.deepEqual(copied, [other], `el portapapeles debería recibir el nombre de la rama: ${JSON.stringify(copied)}`);

    // --- «Ver el diff contra la actual»: git local, sin red; se pinta en el panel derecho.
    await openBranchMenuByRightClick(other);
    const diffItem = await branchMenuItem('diff');
    assert.equal(diffItem.disabled, false, '«Ver el diff» debería estar disponible');
    await clickTestId('branch-menu-diff');
    await waitTestId('branch-diff', { timeout: 20_000 });

    const diff = await getBrowser().execute(() => {
      const el = document.querySelector('[data-testid="branch-diff"]');
      return {
        label: el?.getAttribute('aria-label') ?? '',
        empty: !!document.querySelector('[data-testid="branch-diff-empty"]'),
        files: document.querySelectorAll('[data-testid="branch-diff"] ul.bfiles li').length
      };
    });
    assert.ok(diff.label.includes(other), `el diff debería citar la rama comparada: ${diff.label}`);
    assert.ok(diff.label.includes(current), `el diff debería citar la rama actual: ${diff.label}`);
    // Las ramas del fixture apuntan al mismo commit, así que lo normal es «no hay cambios»;
    // si alguna prueba anterior dejó un commit, habrá filas. Las dos cosas son válidas.
    assert.ok(diff.empty || diff.files > 0, 'el diff debería decir que no hay cambios o listar archivos');

    // Se cierra sin tocar el repositorio.
    await clickTestId('branch-diff-close', { timeout: 10_000 });
    await getBrowser().waitUntil(async () => !(await hasTestId('branch-diff')), {
      timeout: 10_000,
      interval: 100,
      timeoutMsg: 'el diff no se cerró'
    });
  });
});
