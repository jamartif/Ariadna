//! Comparación de capturas PNG con umbral de píxeles y generación de diff.

import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

import pixelmatch from 'pixelmatch';
import { PNG } from 'pngjs';

/** Umbral por píxel (0-1): cuánto puede cambiar un canal antes de contar como distinto. */
export const PIXEL_THRESHOLD = Number(process.env.VISUAL_PIXEL_THRESHOLD ?? 0.18);
/** Fracción de píxeles distintos tolerada sobre el total (antialias de fuentes). */
export const MAX_DIFF_RATIO = Number(process.env.VISUAL_MAX_DIFF_RATIO ?? 0.03);

export function decode(buffer) {
  return PNG.sync.read(buffer);
}

/**
 * Compara dos PNG. Devuelve métricas y, si hay diferencias, escribe el diff.
 * @returns {{ ok: boolean, diffPixels: number, totalPixels: number, ratio: number, sizeMismatch: boolean, diffPath?: string }}
 */
export function comparePng(actualBuffer, baselinePath, diffPath) {
  const baseline = decode(readFileSync(baselinePath));
  const actual = decode(actualBuffer);

  const totalPixels = Math.max(baseline.width * baseline.height, actual.width * actual.height);
  if (baseline.width !== actual.width || baseline.height !== actual.height) {
    return {
      ok: false,
      diffPixels: totalPixels,
      totalPixels,
      ratio: 1,
      sizeMismatch: true,
      message: `tamaño distinto: base ${baseline.width}×${baseline.height}, actual ${actual.width}×${actual.height}`
    };
  }

  const diff = new PNG({ width: actual.width, height: actual.height });
  const diffPixels = pixelmatch(baseline.data, actual.data, diff.data, actual.width, actual.height, {
    threshold: PIXEL_THRESHOLD,
    includeAA: false,
    diffColor: [255, 0, 96]
  });
  const ratio = diffPixels / (actual.width * actual.height);

  let written;
  if (diffPixels > 0) {
    mkdirSync(path.dirname(diffPath), { recursive: true });
    writeFileSync(diffPath, PNG.sync.write(diff));
    written = diffPath;
  }

  return {
    ok: ratio <= MAX_DIFF_RATIO,
    diffPixels,
    totalPixels: actual.width * actual.height,
    ratio,
    sizeMismatch: false,
    diffPath: written,
    message: `${diffPixels} px distintos (${(ratio * 100).toFixed(2)} %, tolerado ${(MAX_DIFF_RATIO * 100).toFixed(1)} %)`
  };
}

export function writePng(file, buffer) {
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, buffer);
}
