# Diseño v2 — riel de ramas, vistas centrales y detalle

Rediseño del armazón de la interfaz: la gestión de ramas pasa a un **riel persistente a la
izquierda**, el centro se queda con las vistas de trabajo y el panel derecho mantiene el
detalle/diff con cabecera fija. Sustituye la maqueta de «barra de título con pestañas +
columna de ramas + pestañas del centro + diff».

Capturas en `specs/rust/diseno-v2/`: `antes-*` (maqueta anterior) y `v2-*` (esta), incluidos
`v2-1440-riel-inspector`, `v2-1440-historial`, `v2-1440-riel-contraido`, `v2-1440-paleta-ramas` y
`v2-1440-forced-colors`.

## Diagnóstico de la maqueta anterior

| Problema | Consecuencia |
|----------|--------------|
| Tres capas de navegación (pestañas de repo, lista de ramas, pestañas Cambios/Historial/Ramas/Stashes/PR) | La misma rama aparecía en dos columnas y «Ramas» duplicaba la lista del sidebar |
| Pestaña «Ramas» con lista + detalle + crear + remotos + etiquetas | Para cambiar de rama hacía falta ir al centro; el árbol del sidebar solo seleccionaba |
| «Ramas» era una vista más, no una herramienta | Al cambiar a Historial o Stashes la gestión de ramas desaparecía |
| Barra de título con pestañas anchas y buscador de 200 px | Poca información de contexto (rama actual) y ruido en la zona de arrastre |
| Columna central de 400 px con pestañas subrayadas | Pestañas y barra de herramientas competían en la misma franja |

## Decisiones

1. **Barra superior compacta** (`TitleBar`): marca, pestañas de repositorio (necesarias para
   multi-repo), separador y **chip de la rama actual** («RAMA», nombre, `↑`/`↓`).
   El chip es la acción de cambio rápido: al pulsarlo ejecuta `rail.focus` y lleva el foco al
   buscador del riel (`Ctrl/⌘+Mayús+B`). Después, paleta, tema, densidad, ajustes y controles
   de ventana. La pestaña de repo sigue siendo el selector de repositorio; el chip es contexto
   del repo activo.
2. **Riel de ramas siempre visible** (`BranchRail`, 264 px; 208–360 redimensionable; 52 px
   contraído; ancho y estado persistidos en `localStorage: ariadna.layout.v2`):
   - tarjeta de la **rama actual** destacada, con seguimiento (`origin/x`) y traer/bajar/subir
     con contadores ahead/behind;
   - **filtro instantáneo** siempre a la vista (`Ctrl/⌘+Mayús+B` lleva el foco; `Enter` cambia
     a la primera coincidencia, `Esc` limpia);
   - árbol **Local / Remoto** plegable, agrupado por prefijo (`feature/`, `origin/`);
   - **inspector contextual** al pulsar una rama: cambiar, fusionar, renombrar, borrar y
     «Detalle» (abre `BranchDetail` en el panel derecho). Sin selección no hay inspector: la
     tarjeta ya resume la actual, evitando duplicarla;
   - contraído: tira de iconos (expandir, nueva rama, rama actual, buscar, remotos) con
     nombres accesibles y tooltips, así el riel nunca desaparece.
3. **Centro sin «Ramas»** (`Workspace`): control segmentado **Cambios · Historial · Stashes ·
   Pull requests** (+ Conflictos cuando hay integración). La vista Cambios conserva lista +
   barra de commit; cada vista mantiene su estado vacío. Se eliminó la pestaña «Ramas» porque
   toda su gestión vive en el riel; el detalle de rama es un modo temporal del panel derecho,
   no una vista. Ganancia: una sola jerarquía (riel → vista → detalle) y la rama actual
   siempre accesible.
4. **Detalle a la derecha**: se conservan `DiffView`, `HistoryDetail`, `StashDetail`,
   `PrDetail` y `ConflictEditor` (cabeceras fijas y acciones por hunk/línea ya existentes).
   El inspector del riel añade una cabecera «Detalle de la rama · Volver al detalle» que se
   cierra al cambiar de vista o al seleccionar otro archivo.
5. **Remotos y etiquetas** salen del centro: `RepoDialog` (modal, trampa de foco, Escape)
   reutiliza `RemotesSection` + `TagsSection` y se abre desde la cabecera del riel. No se
   pierde ninguna operación al eliminar la vista «Ramas».
6. **Ventanas estrechas**: a < 840 px CSS el riel se contrae solo y el centro se apila sobre
   el detalle (una columna), manteniendo el riel de iconos a la izquierda. A 1000×700 el
   diseño completo sigue visible (capturas `v2-1000-*`).
7. **Densidad y jerarquía**: centro por defecto 560 px (340–820, divisor), detalle flexible.
   Cabecera de vista de 44 px con control segmentado; el resumen textual se descartó para no
   duplicar los contadores que ya llevan las pestañas, la lista y la barra de commit.

### Tokens nuevos (`ui/src/lib/styles/tokens.css`)

| Token | Claro | Oscuro | Uso |
|-------|-------|--------|-----|
| `--bg-rail` | `#f2f5fa` | `#0a0e14` | Superficie del riel (entre `bg-app` y `bg-surface`) |

Los anchos del riel (264 px por defecto, 208–360 redimensionable, 52 px contraído) son
constantes de maqueta en `Workspace.svelte` porque el usuario los cambia y se persisten; no
tiene sentido exponerlos como token estático.

Contraste medido de los pares nuevos (WCAG 2.2, texto ≥ 4,5:1):

| Par | Claro | Oscuro |
|-----|-------|--------|
| `text` sobre `bg-rail` | 16,48:1 | 16,43:1 |
| `text-muted` sobre `bg-rail` | 7,46:1 | 8,69:1 |
| `text-subtle` sobre `bg-rail` | 5,65:1 | 6,86:1 |
| `accent` sobre `bg-rail` | 5,35:1 | 10,70:1 |

`tokens.test.ts` exige que claro y oscuro declaren exactamente las mismas variables: `--bg-rail`
está en los tres bloques (claro, oscuro explícito y oscuro del sistema).

### Atajos y paleta

| Atajo | Id | Ámbito | Efecto |
|-------|----|--------|--------|
| `Ctrl/⌘+B` | `rail.toggle` | pestaña | Contrae/expande el riel |
| `Ctrl/⌘+Mayús+B` | `rail.focus` | pestaña | Foco al buscador de ramas |

Ambos se registran en el **registro único** (`shortcuts/registry.ts`) y aparecen en la paleta
con su combo: «Buscar rama…», «Mostrar u ocultar el riel de ramas», además de «Mostrar
Stashes». `findConflicts` sigue sin colisiones; el menú nativo no cambia (no se tocó Rust).

### Accesibilidad

- Riel: `nav` «Árbol de ramas», grupos con `aria-expanded`, filas con foco móvil (una sola
  parada de tabulación, ↑/↓/Inicio/Fin), `aria-current` en la rama actual, texto `sr-only`
  «(rama actual)» y contadores, iconos con `aria-label` propio.
- Cabecera de vistas: `tablist` + `tab`/`tabpanel` con `aria-controls`, foco móvil y flechas.
- Inspector: `group` con nombre, botón «Quitar la selección», acciones con etiqueta visible.
- `forced-colors`: contorno `Highlight` para rama actual, fila seleccionada, tarjeta actual,
  pestaña de vista activa y tarjeta de rama; bordes `ButtonText` en el chip de la barra.
- Foco visible global (`:focus-visible` con `--focus-ring`), sin depender solo del color.
- axe (jsdom) en riel expandido, riel contraído, diálogo de remotos y espacio de trabajo.
- axe-core 4.14 **en la app real** (Vite + mock, Chrome/WebView): **0 violaciones** a
  1440×900, 1000×700 y 720×450, en claro y oscuro, y con el riel contraído (7 configuraciones). La auditoría encontró y se
  corrigieron: `aria-required-children` en el `tablist` de la barra de título (los botones de
  cerrar quedan `aria-hidden`; el teclado cierra con `Supr` o `Ctrl/⌘+W` sobre la pestaña),
  `label-content-name-mismatch` en la paleta, el chip de rama y las pestañas de repositorio
  (el nombre accesible empieza por el texto visible y el detalle va en un `sr-only`; el
  `aria-label` detallado pasa a ser el tooltip), `landmark-one-main`/`page-has-heading-one`
  (App envuelve el `tabpanel` en `<main>` y el espacio de trabajo lleva un `h1` solo para
  lectores), `region` en los divisores y `landmark-unique` en Stashes (`StashDetail` usa
  «Detalle del stash»).

## Pruebas

- `BranchRail.test.ts` (14): lista y agrupación, filtro, plegado, inspector, cambiar con
  aviso de cambios, crear con validación, renombrar, borrar con confirmación nativa, fusionar
  (sin y con conflictos), rama remota, `Ctrl+Shift+B`, flechas/Esc, contraído y contraer.
- `Workspace.test.ts` (6): riel siempre visible, ausencia de la vista «Ramas», navegación por
  flechas entre vistas, cambio de vista con detalle a la derecha, detalle de rama temporal,
  `Ctrl+B`, diálogo de remotos.
- Adaptados: `a11y.test.ts` (riel expandido/contraído, diálogo), `SettingsDialog.test.ts`,
  `StashesPanel.test.ts`, `RemotesSection.test.ts`, `TagsSection.test.ts`.
- Eliminados con la maqueta anterior: `Sidebar.svelte`, `BranchesPanel.svelte` y
  `BranchesPanel.test.ts` (su cobertura se trasladó al riel).

Resultado: `npm run check` (0 errores, 0 avisos), `npm test` (`743` pruebas, todas en verde) y
`npm run build` en verde. `scripts/contrast.mjs --fail`: 0 de 48 pares por debajo de AA.

`vite.config.ts` fija `publicDir: false`: no hay recursos estáticos propios y rolldown-vite
fallaba al intentar copiar un `public/` inexistente (el build dejaba de ser reproducible).

## Pendiente

- Lector de pantalla real (NVDA/VoiceOver) sin probar; axe en jsdom no sustituye la prueba manual.
- axe contra la app Tauri real (E2E) sigue pendiente.
- El riel no lista stashes ni tags a propósito (viven en su vista central y en el diálogo);
  si se pide acceso directo, cabría una sección plegable reutilizando `stashOf`.
- A 720×450 (zoom 200 %) el apilado deja la vista en 44 % de altura: el formulario de commit
  ocupa casi todo el espacio y la lista de archivos queda por encima con poco aire. Funciona
  con scroll y el riel de iconos sigue accesible, pero convendría plegar el compositor en
  ventanas muy bajas.
