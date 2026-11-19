import { describe, expect, it } from 'vitest';
import { leafLabel } from './branchTree';

describe('etiqueta corta de una rama en el árbol del riel', () => {
  it('quita el prefijo de los nodos padre y deja solo su segmento', () => {
    // El caso del informe: bajo `origin/` y `feature/`, la fila se llama `rama`.
    expect(leafLabel('origin/feature/rama', 'origin/feature/')).toBe('rama');
    expect(leafLabel('feature/command-palette', 'feature/')).toBe('command-palette');
    expect(leafLabel('origin/release/1.0/hotfix', 'origin/release/1.0/')).toBe('hotfix');
    expect(leafLabel('origin/release/1.0', 'origin/release/')).toBe('1.0');
  });

  it('en la raíz (sin prefijo) deja el nombre tal cual', () => {
    expect(leafLabel('main', '')).toBe('main');
    expect(leafLabel('origin/main', '')).toBe('origin/main');
  });

  it('nunca deja la etiqueta vacía', () => {
    // Nombres que terminan en barra: el último segmento es vacío.
    expect(leafLabel('origin/feature/', 'origin/feature/')).toBe('feature');
    expect(leafLabel('feature/', 'feature/')).toBe('feature');
    // Prefijo que no encaja (no debería pasar): se ve el nombre completo, no una fila en blanco.
    expect(leafLabel('otra/rama', 'origin/')).toBe('otra/rama');
  });

  it('solo quita el prefijo exacto', () => {
    // `origin/feature` cuelga de `origin/`: su segmento es `feature`, no `origin/feature`.
    expect(leafLabel('origin/feature', 'origin/')).toBe('feature');
    // Y un prefijo parcial no recorta de más: solo se quita el prefijo exacto.
    expect(leafLabel('origin/featureX', 'origin/feature/')).toBe('origin/featureX');
  });
});
