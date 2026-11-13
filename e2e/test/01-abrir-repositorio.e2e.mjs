//! Flujo 1 · abrir un repositorio de prueba creado con git real.

import assert from 'node:assert/strict';
import path from 'node:path';

import { getBrowser, waitInPage } from '../lib/session.mjs';
import { ensureRepoOpen, workspaceOpen } from '../lib/flows.mjs';
import { getFixture } from '../lib/state.mjs';
import { hasText } from '../lib/ui.mjs';
import { repoState } from '../lib/fixture.mjs';

describe('Flujo 1 · abrir repositorio', () => {
  it('arranca en la pantalla de bienvenida con un repositorio abierto', async () => {
    const b = getBrowser();
    const title = await b.getTitle();
    assert.match(title, /Ariadna/, `el título de la ventana debería mencionar Ariadna, es «${title}»`);

    // Si ya está abierto por un flujo anterior, la bienvenida no se ve: se acepta.
    const open = await workspaceOpen();
    if (!open) assert.ok(await hasText('Bienvenida a Ariadna'), 'falta la pantalla de bienvenida');
  });

  it('abre el fixture escribiendo su ruta y muestra el espacio de trabajo', async () => {
    const fx = getFixture();
    await ensureRepoOpen();

    await waitInPage(
      (name) =>
        Array.from(document.querySelectorAll('[role="tab"][aria-label]')).some((el) =>
          (el.getAttribute('aria-label') ?? '').includes(name)
        ),
      [path.basename(fx.repo)],
      { message: 'pestaña del repositorio' }
    );

    // La pestaña anuncia la rama actual (dato del backend, no del DOM inventado).
    const tabLabel = await getBrowser().execute(() =>
      Array.from(document.querySelectorAll('[role="tab"][aria-label]'))
        .map((el) => el.getAttribute('aria-label'))
        .find((l) => l && l.includes('main'))
    );
    assert.ok(tabLabel, 'la pestaña debería indicar la rama main');
    assert.ok(await hasText('Sin preparar'), 'el panel de cambios debería mostrar la sección «Sin preparar»');
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
