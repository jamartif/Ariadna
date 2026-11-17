//! Flujo 11 · el botón de tema de la barra de título alterna claro ↔ oscuro sin desaparecer.
//!
//! Regresión: el botón pasaba por un tercer estado («sistema») en el que el icono cambiaba a un
//! monitor y parecía que el botón se había perdido. Ahora alterna dos estados, el icono muestra
//! la acción y «sistema» se elige en Ajustes o en la paleta.

import assert from 'node:assert/strict';

import { getBrowser } from '../lib/session.mjs';
import { ensureRepoOpen } from '../lib/flows.mjs';
import { currentTheme } from '../lib/ui.mjs';

const BOTON = '[data-testid="theme-toggle"]';

/** Estado visible del botón de tema, tal como lo ve el usuario. */
async function estado() {
  const b = getBrowser();
  return b.execute((sel) => {
    const el = document.querySelector(sel);
    if (!el) return { existe: false };
    const r = el.getBoundingClientRect();
    const cs = getComputedStyle(el);
    const svg = el.querySelector('svg');
    const rsvg = svg ? svg.getBoundingClientRect() : null;
    return {
      existe: true,
      etiqueta: el.getAttribute('aria-label') ?? '',
      visible: r.width > 0 && r.height > 0 && cs.visibility === 'visible' && cs.display !== 'none',
      opacidad: Number(cs.opacity),
      ancho: Math.round(r.width),
      alto: Math.round(r.height),
      icono: svg ? { ancho: Math.round(rsvg.width), alto: Math.round(rsvg.height), trazo: getComputedStyle(svg).stroke } : null,
      tema: document.documentElement.getAttribute('data-theme')
    };
  }, BOTON);
}

describe('Flujo 11 · botón de tema', () => {
  it('alterna claro ↔ oscuro y sigue visible en cada paso', async () => {
    await ensureRepoOpen();
    const b = getBrowser();

    const vistos = [];
    for (let i = 0; i < 6; i++) {
      const antes = await estado();
      assert.equal(antes.existe, true, `clic ${i}: el botón de tema no está en el DOM`);
      assert.equal(antes.visible, true, `clic ${i}: el botón de tema no está visible (${JSON.stringify(antes)})`);
      assert.ok(antes.ancho >= 24 && antes.alto >= 24, `clic ${i}: área de clic pequeña (${antes.ancho}×${antes.alto})`);
      assert.ok(antes.opacidad > 0.5, `clic ${i}: el botón está desvanecido (opacidad ${antes.opacidad})`);
      assert.ok(antes.icono && antes.icono.ancho > 0 && antes.icono.alto > 0, `clic ${i}: el icono no se dibuja`);
      assert.notEqual(antes.icono.trazo, 'none', `clic ${i}: el icono no tiene color de trazo`);

      vistos.push(antes.tema ?? 'system');

      await b.execute((sel) => document.querySelector(sel)?.click(), BOTON);
      // El tema se aplica al instante en el `<html>`; se espera a que cambie de verdad.
      const esperado = vistos[i] === 'dark' ? 'light' : 'dark';
      await b.waitUntil(
        async () => (await currentTheme()) === esperado,
        { timeout: 5_000, interval: 100, timeoutMsg: `clic ${i}: el tema no pasó a ${esperado}` }
      );
    }

    // Nunca se pasa por «sistema» desde el botón: son dos estados que se alternan.
    assert.ok(!vistos.includes('system'), `el botón no debe pasar por «sistema»: ${vistos.join(' → ')}`);
    for (let i = 1; i < vistos.length; i++) {
      assert.notEqual(vistos[i], vistos[i - 1], `clic ${i}: el tema no cambió (${vistos.join(' → ')})`);
    }
  });
});
