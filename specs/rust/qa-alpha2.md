# QA visual y de accesibilidad — alpha.2

Revisión en navegador (Vite + backend simulado) de Pull requests, Conflictos, Ajustes, Ramas/Remotos, Historial y paleta de comandos.
Viewports 1440×900 y 1000×700, tema claro y oscuro, `forced-colors: active` y `prefers-reduced-motion: reduce` emulados por CDP.
Escenarios del mock: `pr-auth`, `pr-rate-limit`, `pr-network`, `pr-empty`.

Método: recorrido manual de cada flujo + un script de auditoría inyectado (contraste aproximado, texto recortado, claves i18n visibles, controles sin nombre, orden de tabulación y foco visible) + axe-core en jsdom (`ui/src/lib/a11y.test.ts`).
Errores de consola durante toda la sesión: ninguno. Claves i18n visibles: ninguna. Controles sin nombre accesible: ninguno.

Capturas en `specs/rust/qa-alpha2/`.

## Corregido (14)

| # | Defecto | Arreglo | Captura |
|---|---------|---------|---------|
| 1 | Con la columna central a 400 px la pestaña «Pull requests» quedaba cortada («Pull reques») y la barra hacía scroll oculto | `Workspace`: container query; con ≤ 440 px se ocultan los iconos y se reduce el relleno | 01 |
| 2 | Error `AUTH` en la lista de PR: el mensaje se aplastaba a una letra por línea y los botones tapaban el texto | `Banner`: `flex-wrap`, el cuerpo tiene base mínima y las acciones bajan | 02 |
| 3 | Detalle de PR a 1000 px: título en columna de 2 palabras, rama partida en tres líneas, botones apretados | `PrDetail`: cabecera con `flex-wrap`, ramas sin saltos entre icono y nombre | 03 |
| 4 | Editor de conflictos a 1000 px: «Marcar como resuelto» cortado y con el icono encima del texto | `ConflictEditor`: pie con `flex-wrap`, botón sin encogerse | 04 |
| 5 | Formularios Revisar/Completar no movían el foco al abrirse ni lo devolvían al botón; Escape no los cerraba | `PrDetail`: foco en el primer control, retorno al botón y Escape (prueba en `a11y.test.ts`) | 05 |
| 6 | Pestaña «Pull requests» y buscador: placeholder «Buscar por título, número, autor o rama» cortado | Placeholder corto (`pr.search.placeholder`); el `aria-label` conserva el texto completo; la barra de herramientas hace `wrap` | 01 |
| 7 | Separadores « · » desaparecían («Ana· hace 10 días», «Página 1· 3 pull requests»): Svelte recorta el espacio inicial dentro de `{#if}` | Separador explícito `{' · '}` | — |
| 8 | Estado de fusión mostraba `clean`/`dirty` crudo del proveedor (sin traducir) junto al texto | Solo se muestra el detalle cuando no es `clean` | — |
| 9 | Diálogo de Ajustes cambiaba de altura al cambiar de sección (486 → 790 px) | Altura estable `min(640px, 88vh)` | — |
| 10 | `forced-colors`: botón primario deshabilitado ilegible (texto negro sobre `Highlight`) | `base.css`: botones deshabilitados con `GrayText`/`ButtonFace` | — |
| 11 | `forced-colors`: selección de fila/archivo/pestaña/filtro solo por color de fondo | Contorno `Highlight` en PR, archivos de PR, conflictos, hilos, bloque resaltado y pestañas | — |
| 12 | ARIA (axe, crítico): `aria-rowcount` en un `listbox` (lista de commits) | `VirtualList` solo lo emite en `grid`/`table`/`treegrid` | — |
| 13 | ARIA (axe, crítico): `<li>` dentro de `ul[role=listbox]` (archivos del commit) | `li role="none"` | — |
| 14 | ARIA (axe, serio): `aria-expanded` en `role="row"` de un `grid` (lista de cambios) | `aria-expanded` pasa a la celda | — |

## Pruebas añadidas

- `vitest-axe` + `axe-core` (devDependencies, versiones fijas).
- `ui/src/test/axe.ts`: helper que falla con violaciones `serious`/`critical`; desactiva `color-contrast` (jsdom no calcula colores) y las reglas de landmarks para componentes sueltos.
- `ui/src/lib/a11y.test.ts` (19 pruebas): PR (lista, vacía, 3 errores, detalle con archivos/conversación, crear/votar/completar), Conflictos (lista, editor de 3 paneles, binario), Ajustes (5 secciones), Ramas, añadir remoto, Historial, paleta, espacio de trabajo y la prueba de foco de los formularios de PR.

## Pendiente / no verificado

- **NVDA y VoiceOver: no se han probado.** axe en jsdom no sustituye a un lector de pantalla.
- Contraste AA: medido con `ui/scripts/contrast.mjs` y documentado en `specs/rust/contraste.md` (48 pares, 0 por debajo de AA).
- axe contra la app real (E2E con `tauri-driver`): sin hacer.
- «Cerrar pull request» no pide confirmación (es reversible con «Reabrir»); decisión de producto.
- Orden de tabulación: correcto en PR (crear/revisar/completar), conflictos y ajustes. El editor de conflictos tiene paneles de solo lectura focusables con nombre propio (intencional).
- Con `prefers-reduced-motion` los iconos de carga no giran (regla global de `base.css`); queda un icono estático. Aceptable, pero conviene un indicador textual donde la carga dure.

## Segunda pasada (accesibilidad y pulido)

Recorrido a 1440×900, 1000×700 y 720×450 CSS (zoom 200 % equivalente), claro y oscuro, y
`forced-colors: active` emulado por CDP. Capturas 15–21.

| # | Defecto | Arreglo | Captura |
|---|---------|---------|---------|
| 15 | `forced-colors`: la rama actual del sidebar y varias selecciones (ramas, commits, stashes) dependían del fondo | Contorno `Highlight` en la rama actual y las filas pulsadas (`Sidebar`, `BranchesPanel`, `HistoryPanel`, `StashesPanel`) | 15, 20 |
| 16 | `forced-colors`: el degradado del botón primario se aplanaba y el relleno desaparecía | `background: Highlight` + `background-image: none` + `forced-color-adjust: none` para los estados normal, hover y activo | 20 |
| 17 | `forced-colors`: marcas de proveedor con degradado ilegibles | Recuadro con `ButtonFace`/`ButtonText` y `forced-color-adjust: none`; puntos de estado con `CanvasText` | 21 |
| 18 | Iniciales de avatar a 8–9 px y etiquetas de estado a 10 px | Mínimo 11 px (`--text-2xs`) en avatares de revisores e hilos, iniciales de proveedor y cajas de estado de 20 px en `PrFiles` y `ChangesPanel` | — |
| 19 | Lista de PR vacía: el panel derecho repetía «Elige una pull request» y un segundo botón | Con la lista cargada y vacía el detalle muestra solo el lienzo vacío, sin texto ni acción | 17 |
| 20 | A 1000×700 el visor de diff quedaba en 160 px y no había scroll | `PrFiles`: cabecera fija (lista + barra), panel desplazable y diff con `min-height: clamp(280px, 40vh, 480px)` | 18 |
| 21 | Zoom 200 % (720×450 CSS): diff recortado, SHA del commit desbordando y paneles de conflicto de 110 px | Lista de archivos en tira horizontal y cabecera/acciones compactas con poca altura; SHA con `word-break`; paneles de conflicto de 220 px con scroll horizontal propio; columna central ≤ 50 % | 19 |

Contraste: además de los pares de `tokens.test.ts`, se midieron el primario (plano, degradado y
hover) y las marcas de proveedor. Se corrigieron `--accent-grad-from` en claro (`#0b7f9a` →
`#0a748c`, porque el hover bajaba a 4,13:1) y los degradados de Azure/GitLab/Bitbucket; informe
completo en `specs/rust/contraste.md`.

Pruebas añadidas: axe de la barra lateral; estado vacío de PR sin duplicado; estructura de la
cabecera fija de «Archivos» y reglas CSS de altura/scroll del diff (+4 pruebas).

