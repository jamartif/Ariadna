import { describe, expect, it } from 'vitest';
import { dominantEol, hasMarkers, locateLines, parseMarkers, resolveAllRegions, resolveRegion, splitLines, withEol } from './conflictMarkers';

const DIFF3 = 'line1\n<<<<<<< HEAD\nfrom-main\n||||||| base\nline2\n=======\nfrom-feature\n>>>>>>> feature\nline3';

describe('conflictMarkers', () => {
  it('analiza diff3 y resuelve cada lado', () => {
    const p = parseMarkers(DIFF3);
    expect(p.regions).toHaveLength(1);
    expect(p.regions[0]).toMatchObject({ ours: 'from-main\n', base: 'line2\n', theirs: 'from-feature\n', line: 1 });
    expect(resolveRegion(DIFF3, 0, 'ours')).toBe('line1\nfrom-main\nline3');
    expect(resolveRegion(DIFF3, 0, 'theirs')).toBe('line1\nfrom-feature\nline3');
    expect(resolveRegion(DIFF3, 0, 'both')).toBe('line1\nfrom-main\nfrom-feature\nline3');
    expect(resolveRegion(DIFF3, 0, 'base')).toBe('line1\nline2\nline3');
  });

  it('estilo de dos vías: sin base, y «base» no borra nada', () => {
    const t = '<<<<<<< HEAD\na\n=======\nb\n>>>>>>> x\n';
    expect(parseMarkers(t).regions[0]?.base).toBeNull();
    expect(resolveRegion(t, 0, 'base')).toBeNull();
    expect(resolveAllRegions(t, 'base')).toBe(t);
    expect(resolveAllRegions(t, 'ours')).toBe('a\n');
  });

  it('varios bloques: resuelve uno y los ids se recalculan', () => {
    const t = 'x\n<<<<<<< a\n1\n=======\n2\n>>>>>>> b\nmid\n<<<<<<< a\n3\n=======\n4\n>>>>>>> b\ny\n';
    const once = resolveRegion(t, 1, 'theirs') as string;
    expect(once).toBe('x\n<<<<<<< a\n1\n=======\n2\n>>>>>>> b\nmid\n4\ny\n');
    expect(hasMarkers(once)).toBe(true);
    expect(parseMarkers(once).regions).toHaveLength(1);
    expect(hasMarkers(resolveAllRegions(t, 'ours'))).toBe(false);
  });

  it('conserva CRLF y el BOM', () => {
    const t = '\uFEFF<<<<<<< HEAD\r\nours\r\n=======\r\ntheirs\r\n>>>>>>> f\r\ntail\r\n';
    expect(resolveAllRegions(t, 'theirs')).toBe('\uFEFFtheirs\r\ntail\r\n');
    expect(resolveRegion('l1\r\n<<<<<<< a\r\n1\r\n=======\r\n2\r\n>>>>>>> b\r\nl3\r\n', 0, 'both')).toBe('l1\r\n1\r\n2\r\nl3\r\n');
  });

  it('marcadores de 8 caracteres o pegados a texto no cuentan', () => {
    expect(hasMarkers('<<<<<<<< no\n<<<<<<<x\n=======\n>>>>>>>\n')).toBe(false);
  });

  it('marcadores sin cerrar se detectan y no se truncan', () => {
    const t = 'a\n<<<<<<< x\nours\n=======\ntheirs\n';
    const p = parseMarkers(t);
    expect(p.unterminated).toBe(true);
    expect(hasMarkers(t)).toBe(true);
    expect(resolveRegion(t, 0, 'ours')).toBeNull();
    expect(resolveAllRegions(t, 'ours')).toBe(t);
  });

  it('locateLines encuentra el fragmento a partir de una línea', () => {
    const c = 'a\nb\nc\nb\nc\nd\n';
    expect(locateLines(c, 'b\nc\n')).toEqual({ start: 1, end: 3 });
    expect(locateLines(c, 'b\nc\n', 2)).toEqual({ start: 3, end: 5 });
    expect(locateLines(c, '')).toBeNull();
    expect(locateLines(c, 'zzz')).toBeNull();
  });

  it('splitLines quita el CR y el salto final', () => {
    expect(splitLines('a\r\nb\n')).toEqual(['a', 'b']);
    expect(splitLines('a\nb')).toEqual(['a', 'b']);
    expect(splitLines('')).toEqual([]);
  });

  it('final de línea dominante y restitución tras el textarea', () => {
    expect(dominantEol('a\r\nb\r\n')).toBe('\r\n');
    expect(dominantEol('a\nb\n')).toBe('\n');
    expect(dominantEol('a\r\nb\nc\n')).toBe('\n');
    expect(dominantEol('sin saltos')).toBe('\n');
    expect(withEol('a\nb\r\nc', '\r\n')).toBe('a\r\nb\r\nc');
    expect(withEol('a\r\nb', '\n')).toBe('a\nb');
  });
});
