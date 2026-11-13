# Registro de cambios

Todos los cambios relevantes de Ariadna se documentan aquí.

El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el proyecto
usa [versionado semántico](https://semver.org/lang/es/).

## [Sin publicar]

### Añadido
- Interfaz bilingüe español/inglés: catálogos `en.ts`/`en.*.ts` con exactamente las mismas claves
  que los `es*` (incluidos plurales y parámetros), ajuste `language` (`system` | `es` | `en`) en
  `settings.json` y selector «Idioma» en Ajustes › General que se aplica **al momento** (también
  en el arranque, sin parpadeo, con `<html lang>` actualizado). Las fechas y números usan el
  idioma activo. Tests de paridad de claves, de detección y persistencia del idioma, de cambio de
  idioma en Ajustes y axe de la pantalla en inglés.
- Resolver conflictos con IA: comando `ai_resolve_conflict(tabId, path)` que propone el
  archivo **completo** ya resuelto a partir de las tres versiones (base, nuestra y suya), con
  los mismos filtros y garantías que el resto de la IA (sensibles, binarios, tope de contexto,
  consentimiento por perfil, cancelación) y validación de la propuesta (sin marcadores y sin
  tocar nada fuera de los bloques). Botón ✨ «Resolver con IA» (`Alt+R`) en el editor de
  conflictos, con panel de revisión: justificación, diferencias contra el resultado actual y
  «Usar» (pasa al editor como texto editable, sin escribir en disco), «Regenerar» y
  «Descartar». Nuevos códigos `AI_CONFLICT_NOT_TEXT` y `AI_CONFLICT_INVALID_PROPOSAL`.
- Pruebas de propiedades (`proptest`) de los parsers de `ariadna-git` (status, diff, log,
  ramas, remotos, conflictos, rutas): ninguno entra en pánico con bytes arbitrarios, UTF-8
  inválido o entrada truncada, y `diff → reconstruir` reproduce la entrada byte a byte.
- Pruebas de rendimiento (`#[ignore]`) con repos sintéticos de 50 000 archivos, 10 000
  commits y diffs grandes, y el informe `specs/rust/rendimiento.md` con las medidas contra
  NF-3, NF-4 y NF-5.
- Etiquetas (tags): lista (ligeras y anotadas, con mensaje y commit), crear en HEAD o en un
  commit del historial (con mensaje es anotada), borrar en local, publicar y borrar en el
  remoto (con confirmación nativa), validación del nombre con las reglas de `git
  check-ref-format` y errores tipados. Sección «Etiquetas» en el panel de Ramas y «Crear
  etiqueta aquí» en el detalle de un commit.

### Cambiado
- Preparar o quitar de preparados más de 2 000 rutas usa órdenes de índice lineales
  (`update-index`) en vez de pathspecs: con 50 000 archivos, quitar de preparados pasa de
  ≈ 35 s a ≈ 0,2 s y preparar de ≈ 35 s a ≈ 6 s.
- `diff` con más de 2 000 rutas ya no revienta el límite de argumentos del sistema: pide el
  diff completo y filtra en memoria.
- El historial pagina por cursor en vez de `--skip`: `log_page` acepta un `cursor` opcional y
  cada página cuesta lo mismo a cualquier profundidad (en 100 000 commits la última página
  deja de crecer hasta ≈ 0,5 s). Sin `cursor`, el comportamiento es el de antes.

### Corregido
- Una salida de git de más de 64 MiB (p. ej. un diff enorme) se devolvía cortada a media
  línea como si fuera completa; ahora es un error claro (`IO`).
- QA visual y de accesibilidad de la interfaz (14 defectos) con pruebas axe.

## [0.1.0-alpha.2] - 2026-11-11

### Añadido
- IA alpha.2: perfiles aislados por proveedor, cuentas y hoja de propuesta.
- Backend de pull requests para Azure DevOps y GitHub, y su interfaz, con un spike real
  contra GitHub.

### Corregido
- Pruebas intermitentes: pausa holgada en el flujo de stderr del runner de procesos y
  espera del historial de FSEvents en macOS antes de contar eventos del watcher.
- La CI de gitleaks permite los secretos falsos de los tests de IA y remotos.

## [0.1.0-alpha.1] - 2026-11-10

Primera versión alfa de la reescritura en Rust + Tauri 2 + Svelte (multiplataforma).

### Añadido
- Motor Git sobre el `git` del sistema (`ariadna-git`): abrir repo, status, preparar y
  quitar por archivo y por hunk, commit, enmendar con seguridad, log, búsqueda, diff,
  ramas, merge, resolución de conflictos y remotos; rutas por stdin y validadas.
- Ejecución de procesos (`ariadna-process`): tiempo límite, cancelación, árbol de procesos
  y entorno saneado.
- Aplicación Tauri: pestañas, hibernación, vigilancia de archivos, ajustes, menús nativos,
  clonado con PAT, remotos y botón de IA para el mensaje de commit.
- Contrato IPC con errores tipados y fixtures.

### Cambiado
- La versión Swift/SwiftUI (solo macOS) se retira de `main` (PR #13); se conserva en la
  rama `legacy-swift` y en la etiqueta `swift-final`.

### Corregido
- Se suprime el menú contextual del navegador (Inspeccionar, Recargar…) salvo en campos
  de texto.
- La barra lateral abre el panel de Ramas y la IA devuelve siempre una descripción.

[Sin publicar]: https://github.com/jamartif/Ariadna/compare/408b0fb...HEAD
[0.1.0-alpha.2]: https://github.com/jamartif/Ariadna/compare/v0.1.0-alpha.1...408b0fb
[0.1.0-alpha.1]: https://github.com/jamartif/Ariadna/commit/1ecdd35
