# Contraste medido (WCAG 2.2 AA)

Medición de los pares que no se pueden comprobar con `background-color` en el navegador: el
botón primario (color plano y degradado), el texto sobre superficie elevada y las marcas de
proveedor. La primera pasada de QA los marcó con ratio ≈ 1 porque el degradado no se puede
leer como color de fondo; el informe anterior quedó pendiente de una medición de referencia.

Método y reproducibilidad:

- `ui/scripts/contrast.mjs` (sin dependencias) lee los tokens de `ui/src/lib/styles/tokens.css`
  y extrae los dos extremos del degradado de cada marca de `ui/src/lib/components/ProviderMark.svelte`.
- Luminancia relativa y ratio según WCAG 2.2 (`(L1 + 0,05) / (L2 + 0,05)`).
- Umbrales: 4,5:1 para texto normal y 3:1 para componentes y contorno de foco.
- El degradado se mide en sus dos extremos; cualquier punto intermedio queda entre ambos.
- El hover del botón primario aplica `filter: brightness(1.07)`, que aclara el fondo y es el peor
  caso del estado; se mide también.
- Repetir: `cd ui && node scripts/contrast.mjs` (informe legible), `--md` (tablas) o `--fail`
  (código de salida 1 si algo baja de AA).

`tokens.test.ts` ya cubre el resto de parejas de la paleta (texto, estados de archivo, sintaxis,
diferencias); este informe añade lo que el test no puede medir.

## Correcciones motivadas por la medición

| Elemento | Antes | Después | Motivo |
|----------|-------|---------|--------|
| `--accent-grad-from` (tema claro) | `#0b7f9a` (4,65:1) | `#0a748c` (5,40:1) | El hover (`brightness(1.07)`) bajaba a 4,13:1, por debajo de AA |
| Marca Azure (arriba) | `#2b95e8` (3,20:1) | `#1c6fb0` (5,32:1) | Iniciales blancas sobre el degradado |
| Marca GitLab | `#ff8a4c` → `#d9531a` (2,34:1 / 4,04:1) | `#c2410c` → `#9a3412` (5,18:1 / 7,31:1) | Ídem |
| Marca Bitbucket (arriba) | `#4b95ff` (2,98:1) | `#1f66c9` (5,52:1) | Ídem |

El resto de colores de las marcas (GitHub) ya cumplía y no cambia.

## Tablas medidas


### Tema claro

| Par | Primer plano | Fondo | Ratio | Mínimo | Resultado |
|-----|--------------|-------|------:|-------:|-----------|
| text sobre bg-raised | `#0f1722` | `#ffffff` | 18,01:1 | 4.5:1 | ✔ |
| text-muted sobre bg-raised | `#445064` | `#ffffff` | 8,15:1 | 4.5:1 | ✔ |
| text-subtle sobre bg-raised | `#566275` | `#ffffff` | 6,18:1 | 4.5:1 | ✔ |
| text sobre bg-float | `#0f1722` | `#ffffff` | 18,01:1 | 4.5:1 | ✔ |
| text-muted sobre bg-float | `#445064` | `#ffffff` | 8,15:1 | 4.5:1 | ✔ |
| text-subtle sobre bg-float | `#566275` | `#ffffff` | 6,18:1 | 4.5:1 | ✔ |
| text sobre bg-subtle | `#0f1722` | `#f1f4f9` | 16,34:1 | 4.5:1 | ✔ |
| text-muted sobre bg-subtle | `#445064` | `#f1f4f9` | 7,39:1 | 4.5:1 | ✔ |
| text-subtle sobre bg-subtle | `#566275` | `#f1f4f9` | 5,60:1 | 4.5:1 | ✔ |
| accent-fg sobre accent (plano) | `#ffffff` | `#0b6e87` | 5,84:1 | 4.5:1 | ✔ |
| accent-fg sobre accent-hover | `#ffffff` | `#095a70` | 7,75:1 | 4.5:1 | ✔ |
| accent-fg sobre grad-from (degradado) | `#ffffff` | `#0a748c` | 5,40:1 | 4.5:1 | ✔ |
| accent-fg sobre grad-to (degradado) | `#ffffff` | `#0a6a83` | 6,17:1 | 4.5:1 | ✔ |
| accent-fg sobre grad-from al pasar (hover) | `#ffffff` | `#0b7c96` | 4,84:1 | 4.5:1 | ✔ |
| blanco sobre marca azure (arriba) | `#ffffff` | `#1c6fb0` | 5,32:1 | 4.5:1 | ✔ |
| blanco sobre marca azure (abajo) | `#ffffff` | `#0a64b8` | 5,95:1 | 4.5:1 | ✔ |
| blanco sobre marca github (arriba) | `#ffffff` | `#4a5565` | 7,56:1 | 4.5:1 | ✔ |
| blanco sobre marca github (abajo) | `#ffffff` | `#1f2733` | 15,04:1 | 4.5:1 | ✔ |
| blanco sobre marca gitlab (arriba) | `#ffffff` | `#c2410c` | 5,18:1 | 4.5:1 | ✔ |
| blanco sobre marca gitlab (abajo) | `#ffffff` | `#9a3412` | 7,31:1 | 4.5:1 | ✔ |
| blanco sobre marca bitbucket (arriba) | `#ffffff` | `#1f66c9` | 5,52:1 | 4.5:1 | ✔ |
| blanco sobre marca bitbucket (abajo) | `#ffffff` | `#1d5fd6` | 5,74:1 | 4.5:1 | ✔ |
| border-strong sobre bg-surface | `#707c8f` | `#fbfcfe` | 4,12:1 | 3:1 | ✔ |
| focus-ring sobre bg-surface | `#0b6e87` | `#fbfcfe` | 5,69:1 | 3:1 | ✔ |

### Tema oscuro

| Par | Primer plano | Fondo | Ratio | Mínimo | Resultado |
|-----|--------------|-------|------:|-------:|-----------|
| text sobre bg-raised | `#e8edf3` | `#141a22` | 14,86:1 | 4.5:1 | ✔ |
| text-muted sobre bg-raised | `#a4afbc` | `#141a22` | 7,86:1 | 4.5:1 | ✔ |
| text-subtle sobre bg-raised | `#8f9bab` | `#141a22` | 6,20:1 | 4.5:1 | ✔ |
| text sobre bg-float | `#e8edf3` | `#19212b` | 13,79:1 | 4.5:1 | ✔ |
| text-muted sobre bg-float | `#a4afbc` | `#19212b` | 7,29:1 | 4.5:1 | ✔ |
| text-subtle sobre bg-float | `#8f9bab` | `#19212b` | 5,75:1 | 4.5:1 | ✔ |
| text sobre bg-subtle | `#e8edf3` | `#121821` | 15,14:1 | 4.5:1 | ✔ |
| text-muted sobre bg-subtle | `#a4afbc` | `#121821` | 8,01:1 | 4.5:1 | ✔ |
| text-subtle sobre bg-subtle | `#8f9bab` | `#121821` | 6,32:1 | 4.5:1 | ✔ |
| accent-fg sobre accent (plano) | `#03222a` | `#22d3ee` | 9,18:1 | 4.5:1 | ✔ |
| accent-fg sobre accent-hover | `#03222a` | `#67e1f3` | 10,77:1 | 4.5:1 | ✔ |
| accent-fg sobre grad-from (degradado) | `#03222a` | `#46e1f6` | 10,55:1 | 4.5:1 | ✔ |
| accent-fg sobre grad-to (degradado) | `#03222a` | `#14b9d6` | 7,06:1 | 4.5:1 | ✔ |
| accent-fg sobre grad-from al pasar (hover) | `#03222a` | `#4bf1ff` | 12,10:1 | 4.5:1 | ✔ |
| blanco sobre marca azure (arriba) | `#ffffff` | `#1c6fb0` | 5,32:1 | 4.5:1 | ✔ |
| blanco sobre marca azure (abajo) | `#ffffff` | `#0a64b8` | 5,95:1 | 4.5:1 | ✔ |
| blanco sobre marca github (arriba) | `#ffffff` | `#4a5565` | 7,56:1 | 4.5:1 | ✔ |
| blanco sobre marca github (abajo) | `#ffffff` | `#1f2733` | 15,04:1 | 4.5:1 | ✔ |
| blanco sobre marca gitlab (arriba) | `#ffffff` | `#c2410c` | 5,18:1 | 4.5:1 | ✔ |
| blanco sobre marca gitlab (abajo) | `#ffffff` | `#9a3412` | 7,31:1 | 4.5:1 | ✔ |
| blanco sobre marca bitbucket (arriba) | `#ffffff` | `#1f66c9` | 5,52:1 | 4.5:1 | ✔ |
| blanco sobre marca bitbucket (abajo) | `#ffffff` | `#1d5fd6` | 5,74:1 | 4.5:1 | ✔ |
| border-strong sobre bg-surface | `#6b7889` | `#0d1117` | 4,21:1 | 3:1 | ✔ |
| focus-ring sobre bg-surface | `#22d3ee` | `#0d1117` | 10,47:1 | 3:1 | ✔ |

## Resultado

48 pares medidos, **0 por debajo de AA**. El peor caso es el hover del botón primario en tema
claro (4,84:1) y la marca GitLab en su extremo superior (5,18:1); ambos con margen sobre 4,5:1.

Fuera de este informe: el contraste de los iconos decorativos y de los estados de archivo ya lo
comprueba `tokens.test.ts`; axe en jsdom no calcula color, así que estas tablas son la referencia
de contraste del proyecto.
