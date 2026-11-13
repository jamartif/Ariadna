import axe from 'axe-core';
import { expect } from 'vitest';

/**
 * Auditoría de accesibilidad con axe-core sobre un nodo del DOM de jsdom.
 *
 * - Falla con violaciones de impacto `serious` o `critical`.
 * - `color-contrast` está desactivada: jsdom no calcula estilos de layout ni de color reales, así que
 *   el contraste (AA) se revisa en navegador (specs/rust/qa-alpha2.md) y en `styles/tokens.test.ts`.
 * - `region`/`landmark-*` se desactivan al auditar componentes sueltos: la página completa aporta
 *   los landmarks (se comprueban en el test de la aplicación entera).
 */
export async function expectNoSeriousViolations(root: Element = document.body, extraOff: string[] = []): Promise<void> {
  // axe pinta iconos en un <canvas> para algunas reglas; jsdom no lo implementa (evita el ruido en consola).
  HTMLCanvasElement.prototype.getContext = (() => null) as unknown as HTMLCanvasElement['getContext'];
  const off = ['color-contrast', 'region', 'landmark-one-main', 'page-has-heading-one', ...extraOff];
  const rules = Object.fromEntries(off.map((id) => [id, { enabled: false }]));
  const results = await axe.run(root, { rules, resultTypes: ['violations'] });
  const bad = results.violations.filter((v) => v.impact === 'serious' || v.impact === 'critical');
  const report = bad.map((v) => `${v.id} (${v.impact}): ${v.help}\n${v.nodes.map((n) => `  - ${n.target.join(' ')}\n    ${n.html.slice(0, 160)}`).join('\n')}`).join('\n\n');
  expect(bad, report).toEqual([]);
}
