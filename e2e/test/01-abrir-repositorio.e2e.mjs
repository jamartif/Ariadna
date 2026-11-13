//! Flujo 1 · abrir un repositorio de prueba creado con git real.

import assert from 'node:assert/strict';
import path from 'node:path';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { ensureRepoOpen, showView, welcomeVisible, workspaceOpen } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { fileSection, tabs } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

describe('Flujo 1 · abrir repositorio', () => {
  it('arranca en la pantalla de bienvenida con un repositorio abierto', async () => {
    const b = getBrowser();
    const title = await b.getTitle();
    assert.match(title, /Ariadna/, `el título de la ventana debería mencionar Ariadna, es «${title}»`);

    // Si ya está abierto por un flujo anterior, la bienvenida no se ve: se acepta.
    const open = await workspaceOpen();
    if (!open) assert.ok(await welcomeVisible(), 'falta la pantalla de bienvenida');
  });

  it('abre el fixture escribiendo su ruta y muestra el espacio de trabajo', async () => {
    const fx = getFixture();
    const name = path.basename(fx.repo);
    await ensureRepoOpen();
    await showView('changes');

    // La pestaña del repositorio lleva el nombre y la rama en su propio texto.
    await waitInPage(
      (want) =>
        Array.from(document.querySelectorAll('[role="tab"][data-tab-id]')).some(
          (el) => el.getAttribute('title')?.includes(want) || (el.textContent ?? '').includes(want)
        ),
      [name],
      { message: 'pestaña del repositorio' }
    );

    const repoTab = (await tabs()).find((t) => t.kind === 'repo' && t.text.includes(name));
    assert.ok(repoTab, 'la pestaña del repositorio debería existir');
    assert.match(repoTab.title, /main/, 'la pestaña debería indicar la rama main');
    assert.equal(await fileSection('docs/guia.md'), 'unstaged', 'guia.md debería estar sin preparar');
  });

  it('el estado que muestra la interfaz coincide con git', async () => {
    const fx = getFixture();
    const state = repoState(fx.repo);
    assert.equal(state.branch, fx.branchName);
    assert.equal(state.total, fx.commitsTotales);
    assert.ok(state.unstaged.includes('src/app.js'), `cambios sin preparar inesperados: ${state.unstaged}`);
    assert.ok(state.staged.includes(fx.preparado), `preparados inesperados: ${state.staged}`);
  });
});
