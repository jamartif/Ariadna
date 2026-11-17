//! El botón de tema de la barra de título: dos estados y un icono que anuncia la acción.
//!
//! Regresión real: al pasar por «sistema» el icono se convertía en un monitor y parecía que el
//! botón había desaparecido. Ahora alterna claro ↔ oscuro y «sistema» se elige en Ajustes.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

import { settings } from './settings.svelte';

/** Simula la preferencia del sistema operativo. */
function sistemaOscuro(oscuro: boolean): void {
  vi.stubGlobal(
    'matchMedia',
    vi.fn((q: string) => ({
      matches: q.includes('dark') ? oscuro : !oscuro,
      media: q,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
      addListener: () => undefined,
      removeListener: () => undefined,
      onchange: null,
      dispatchEvent: () => false
    }))
  );
}

describe('botón de tema', () => {
  beforeEach(() => {
    localStorage.clear();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    settings.setTheme('system');
  });

  it('alterna claro → oscuro → claro sin pasar nunca por «sistema»', () => {
    sistemaOscuro(false);
    settings.setTheme('light');
    settings.cycleTheme();
    expect(settings.theme).toBe('dark');
    settings.cycleTheme();
    expect(settings.theme).toBe('light');
  });

  it('desde «sistema» oscuro el primer clic lleva a claro', () => {
    sistemaOscuro(true);
    settings.setTheme('system');
    expect(settings.effectiveTheme).toBe('dark');
    settings.cycleTheme();
    expect(settings.theme).toBe('light');
  });

  it('desde «sistema» claro el primer clic lleva a oscuro', () => {
    sistemaOscuro(false);
    settings.setTheme('system');
    expect(settings.effectiveTheme).toBe('light');
    settings.cycleTheme();
    expect(settings.theme).toBe('dark');
  });

  it('aplica el tema al documento y lo guarda', () => {
    sistemaOscuro(false);
    settings.setTheme('dark');
    expect(document.documentElement.getAttribute('data-theme')).toBe('dark');
    expect(JSON.parse(localStorage.getItem('ariadna.settings.v1') ?? '{}').theme).toBe('dark');
    settings.setTheme('system');
    expect(document.documentElement.hasAttribute('data-theme')).toBe(false);
  });
});
