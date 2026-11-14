# Registro de cambios

Todos los cambios relevantes de Ariadna se documentan aquí.

El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el proyecto
usa [versionado semántico](https://semver.org/lang/es/).

## [Sin publicar]

Lo que hay encima de `v0.1.0-alpha.3`.

### Añadido
- Rediseño de la ventana alrededor de un **riel de ramas persistente** a la izquierda (visible
  en todas las vistas: cambiar, crear, renombrar, borrar, fusionar y traer/subir con
  contadores), vista central con control segmentado (Cambios · Historial · Stashes ·
  Pull requests · Conflictos) y panel derecho con el detalle o el diff. La gestión de ramas
  deja de ser una pestaña del centro.
- **Menú contextual de cada rama del riel**: se abre con clic derecho o con la tecla
  Menú / `Mayús+F10` (navegación con ↑/↓/Inicio/Fin, `Esc` cierra y devuelve el foco a la
  fila), se voltea si no cabe en la ventana y deshabilita con motivo las acciones que no
  aplican. Acciones: cambiar, crear desde esa, fusionar, rebasar, traer, subir, bajar,
  renombrar, borrar en local y en el remoto, copiar nombre y SHA, ver el diff contra la
  actual, comparar con otra rama, abrir en el navegador y crear pull request
  ([`docs/menu-ramas.md`](docs/menu-ramas.md)).
- **Resolver conflictos con IA**: comando `ai_resolve_conflict(tabId, path)` que propone el
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
- **Interfaz bilingüe español/inglés**: catálogos `en.ts`/`en.*.ts` con exactamente las mismas
  claves que los `es*` (incluidos plurales y parámetros), ajuste `language` (`system` | `es` |
  `en`) en `settings.json` y selector «Idioma» en Ajustes › General que se aplica **al
  momento** (también en el arranque, sin parpadeo, con `<html lang>` actualizado). Las fechas
  y números usan el idioma activo. Tests de paridad de claves, de detección y persistencia del
  idioma, de cambio de idioma en Ajustes y axe de la pantalla en inglés.
- **GitHub Copilot deja de ser experimental**: verificado con `GitHub Copilot CLI 1.0.112`
  (2026-11-14) en Windows 11 x64: argumentos finales (`--output-format json --no-ask-user
  --disable-builtin-mcps --disable-client-telemetry --no-auto-update`), prompt por **stdin sin
  `-p`**, salida **JSONL** capturada como fixture real, herramientas denegadas
  (`--available-tools` con un catálogo vacío + `--deny-tool`) y detección del binario. Codex
  sigue marcado como no verificado. Detalle y límites en
  [`docs/ia-copilot.md`](docs/ia-copilot.md).
- **Clonar con la sesión que ya tienes**: además del PAT, el formulario de clonado ofrece la
  sesión iniciada de **GitHub CLI (`gh`)** y **GitLab CLI (`glab`)** —el token se lee del CLI
  en Rust, se usa en memoria con `zeroize` y no cruza el IPC— y el flujo **OAuth por código de
  dispositivo** (RFC 8628) listo pero inactivo mientras no haya `client_id` configurado. Un
  token de GitLab no se manda nunca a GitHub ni al revés (`POLICY`), y `clone_auth_options`
  dice qué métodos hay sin pedir secretos ([`docs/oauth-cli.md`](docs/oauth-cli.md)).
- **E2E y regresión visual en CI**: flujos de extremo a extremo con `tauri-driver` y
  WebKitWebDriver sobre la app empaquetada, incluido el riel y su menú contextual, 16 capturas
  por sistema operativo con `pixelmatch`, job `e2e` en Linux (con modo `update_baselines` que
  publica los baselines como artefacto) y baselines de Linux versionados.
- **Política de firma de código**: README y [`docs/firma-de-codigo.md`](docs/firma-de-codigo.md)
  con la política de SignPath (artefactos, aprobadores y privacidad) y la
  [`docs/solicitud-signpath.md`](docs/solicitud-signpath.md) lista para enviar.
- Documentación nueva: [`docs/menu-ramas.md`](docs/menu-ramas.md) (tabla de acciones del
  menú), [`docs/ia-copilot.md`](docs/ia-copilot.md) (qué está verificado de Copilot),
  [`docs/oauth-cli.md`](docs/oauth-cli.md) (registro de la app OAuth),
  [`docs/privacidad.md`](docs/privacidad.md) y [`docs/windows-sin-instalar.md`](docs/windows-sin-instalar.md);
  el README pasa a estar en inglés y estrena la tabla de verificación.

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

[Sin publicar]: https://github.com/jamartif/Ariadna/compare/v0.1.0-alpha.3...HEAD
[0.1.0-alpha.2]: https://github.com/jamartif/Ariadna/compare/v0.1.0-alpha.1...408b0fb
[0.1.0-alpha.1]: https://github.com/jamartif/Ariadna/commit/1ecdd35
