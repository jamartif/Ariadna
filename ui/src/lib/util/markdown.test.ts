import { describe, expect, it } from 'vitest';
import { inlineText, parseInline, parseMarkdown, safeHttpsUrl, type Block, type Inline } from './markdown';

const kinds = (nodes: Inline[]) => nodes.map((n) => n.t);
const para = (src: string): Inline[] => {
  const b = parseMarkdown(src)[0] as Block;
  if (b.t !== 'p') throw new Error(`se esperaba un párrafo, hay ${b.t}`);
  return b.c;
};

describe('safeHttpsUrl', () => {
  it('solo acepta https sin credenciales', () => {
    expect(safeHttpsUrl('https://example.com/a?b=1')).toBe('https://example.com/a?b=1');
    expect(safeHttpsUrl('http://example.com')).toBeNull();
    expect(safeHttpsUrl('javascript:alert(1)')).toBeNull();
    expect(safeHttpsUrl('data:text/html,<script>1</script>')).toBeNull();
    expect(safeHttpsUrl('file:///etc/passwd')).toBeNull();
    expect(safeHttpsUrl('//example.com')).toBeNull();
    expect(safeHttpsUrl('https://user:pw@example.com')).toBeNull();
    expect(safeHttpsUrl('https://exa mple.com')).toBeNull();
    expect(safeHttpsUrl('  https://example.com\n')).toBe('https://example.com/');
    expect(safeHttpsUrl('')).toBeNull();
  });
});

describe('Markdown → AST', () => {
  it('títulos, párrafos y énfasis', () => {
    const [h, p] = parseMarkdown('## Resumen\n\nTexto con **negrita**, *cursiva* y ~~tachado~~.');
    expect(h).toMatchObject({ t: 'h', level: 2 });
    expect(p?.t).toBe('p');
    const nodes = (p as Extract<Block, { t: 'p' }>).c;
    expect(kinds(nodes)).toEqual(['text', 'strong', 'text', 'em', 'text', 'del', 'text']);
  });

  it('código en línea y bloques con y sin lenguaje', () => {
    expect(para('usa `git status` ahora')[1]).toEqual({ t: 'code', v: 'git status' });
    const blocks = parseMarkdown('```ts\nconst a = 1;\n<b>no es html</b>\n```\n\ndespués');
    expect(blocks[0]).toEqual({ t: 'code', v: 'const a = 1;\n<b>no es html</b>', lang: 'ts' });
    expect(blocks[1]?.t).toBe('p');
    // Un bloque sin cerrar llega hasta el final sin romper nada.
    expect(parseMarkdown('```\nsin cierre')[0]).toMatchObject({ t: 'code', v: 'sin cierre' });
  });

  it('listas ordenadas, con viñetas, anidadas y de tareas', () => {
    const [ul] = parseMarkdown('- uno\n- dos\n  - hijo\n- [x] hecha\n- [ ] pendiente');
    expect(ul).toMatchObject({ t: 'list', ordered: false });
    const items = (ul as Extract<Block, { t: 'list' }>).items;
    expect(items).toHaveLength(4);
    expect(items[1]?.c.some((b) => b.t === 'list')).toBe(true);
    expect(items[2]).toMatchObject({ task: true });
    expect(items[3]).toMatchObject({ task: false });
    const [ol] = parseMarkdown('3. tres\n4. cuatro');
    expect(ol).toMatchObject({ t: 'list', ordered: true, start: 3 });
  });

  it('citas y reglas horizontales', () => {
    const [q, hr] = parseMarkdown('> cita\n> sigue\n\n---');
    expect(q?.t).toBe('quote');
    expect(hr?.t).toBe('hr');
  });

  it('los comentarios HTML (plantillas) no se ven', () => {
    const blocks = parseMarkdown('<!-- explica qué cambia -->\n## Qué cambia\n<!--\nmultilínea\n-->\ntexto');
    expect(blocks.map((b) => b.t)).toEqual(['h', 'p']);
  });

  it('el HTML en bruto queda como texto, nunca como nodo', () => {
    const nodes = para('<script>alert(1)</script> y <img src=x onerror=alert(1)>');
    expect(kinds(nodes)).toEqual(['text']);
    expect(inlineText(nodes)).toContain('<script>alert(1)</script>');
  });

  it('enlaces: solo https; los demás se quedan sin enlace', () => {
    const ok = para('[doc](https://example.com/doc)')[0];
    expect(ok).toMatchObject({ t: 'link', href: 'https://example.com/doc' });
    for (const bad of ['javascript:alert(1)', 'http://example.com', 'data:text/html,x', 'vbscript:x', 'https://u:p@example.com']) {
      const nodes = para(`[clic](${bad})`);
      expect(nodes.some((n) => n.t === 'link'), bad).toBe(false);
      expect(inlineText(nodes)).toMatch(/^clic/);
    }
  });

  it('URL sueltas y <autoenlaces> con https; sin la puntuación final', () => {
    const nodes = para('mira https://example.com/a. Y <https://example.org/b> o http://inseguro.test');
    const links = nodes.filter((n): n is Extract<Inline, { t: 'link' }> => n.t === 'link');
    expect(links.map((l) => l.href)).toEqual(['https://example.com/a', 'https://example.org/b']);
  });

  it('las imágenes no se cargan: salen como enlace con su texto alternativo, o como texto', () => {
    const a = para('![captura](https://example.com/x.png)');
    expect(a).toEqual([{ t: 'link', href: 'https://example.com/x.png', c: [{ t: 'text', v: 'captura' }] }]);
    const b = para('![tracker](http://evil.test/p.gif)');
    expect(kinds(b)).toEqual(['text']);
  });

  it('snake_case y asteriscos sueltos no se interpretan como cursiva', () => {
    expect(kinds(para('mi_variable_larga y 2 * 3 * 4'))).toEqual(['text']);
  });

  it('escapes y saltos duros', () => {
    expect(inlineText(para('\\*no cursiva\\*'))).toBe('*no cursiva*');
    expect(kinds(para('uno  \ndos'))).toEqual(['text', 'br', 'text']);
  });

  it('entradas enormes o anidadas con saña no bloquean ni desbordan', () => {
    const t0 = Date.now();
    parseMarkdown('x'.repeat(500_000));
    parseMarkdown('> '.repeat(500) + 'hondo');
    parseMarkdown('- a\n' + '  - a\n'.repeat(300));
    parseInline('*'.repeat(5000) + '_'.repeat(5000));
    parseInline('['.repeat(3000) + '](https://example.com)'.repeat(10));
    expect(Date.now() - t0).toBeLessThan(5000);
  });

  it('una descripción vacía no produce bloques', () => {
    expect(parseMarkdown('')).toEqual([]);
    expect(parseMarkdown('   \n\n')).toEqual([]);
  });
});
