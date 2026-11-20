import { render } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import Logo from './Logo.svelte';

/**
 * Varios logos conviven en la misma página (barra superior, diálogo de «acerca de», estado vacío).
 * Si el id del degradado es fijo, `url(#…)` puede resolver al degradado de otra instancia.
 */
describe('Logo · id del degradado', () => {
  it('cada instancia usa un id propio y su `fill` apunta al suyo', () => {
    const primero = render(Logo, { props: { size: 24 } }).container.querySelector('svg');
    const segundo = render(Logo, { props: { size: 32 } }).container.querySelector('svg');
    expect(primero).not.toBeNull();
    expect(segundo).not.toBeNull();

    const idPrimero = primero?.querySelector('linearGradient')?.getAttribute('id');
    const idSegundo = segundo?.querySelector('linearGradient')?.getAttribute('id');
    expect(idPrimero).toBeTruthy();
    expect(idSegundo).toBeTruthy();
    expect(idPrimero).not.toBe(idSegundo);

    // Y cada rect usa el id de su propio degradado, no el del primero.
    expect(primero?.querySelector('rect')?.getAttribute('fill')).toBe(`url(#${idPrimero})`);
    expect(segundo?.querySelector('rect')?.getAttribute('fill')).toBe(`url(#${idSegundo})`);
  });
});
