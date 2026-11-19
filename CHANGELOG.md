# Registro de cambios

Todos los cambios relevantes de Ariadna se documentan aquí.

El formato sigue [Keep a Changelog](https://keepachangelog.com/es-ES/1.1.0/) y el proyecto
usa [versionado semántico](https://semver.org/lang/es/).

## [0.1.0-alpha.13] - 2026-11-20

**Respecto a `v0.1.0-alpha.12`:** el **riel izquierdo** gana dos grupos más de referencias, para que
no se quede nada fuera de la vista: las **refs de pull request** descargadas en el repo
(`refs/pull/*`, `refs/merge-requests/*`) y los **otros worktrees** del repositorio — incluidos los que
están en «detached HEAD», cuyos commits no alcanza ninguna rama.

### Añadido
- **Riel › Refs de PR**: grupo plegable con contador; cada fila con el nombre corto (`7/head`), el SHA
  corto y el tooltip con la referencia completa (`refs/pull/7/head`).
- **Riel › Otros worktrees**: grupo plegable con contador; muestra la rama del worktree o, si está en
  «detached HEAD», su SHA corto con la marca «sin rama», y la ruta completa en el tooltip. El worktree
  principal no se repite.
- Los dos grupos respetan el **filtro** del riel y se recargan con cada revisión del repo.

### Por dentro
- Lectura **explícita** `extra_refs` (`src-tauri/src/core/refs_ops.rs`): una sola operación que pide
  refs de PR y worktrees con `read_op`. **No pasa por el snapshot**, así que no añade lecturas al
  refresco de la sesión ni altera las pruebas de sesión con `FakeEngine`.

## [0.1.0-alpha.12] - 2026-11-20

Entra lo que llegó a `main` después de la alpha.11.

**Respecto a `v0.1.0-alpha.11`:** el **historial completo** ya no se deja referencias fuera — recorre
todas las de `refs/` y el `HEAD` de cada worktree (también los «detached»), así que aparecen las ramas
que solo viven en `refs/pull` o en otro worktree; el **riel** lista además las **etiquetas**; el
**diff a dos columnas** gana ajuste al ancho, cabecera de hunk fija y salto al cambio
anterior/siguiente; la **vista de árbol** de Cambios anida cada carpeta con su recuento; la interfaz
se **refresca sola** al mergear una PR y al cambiar de pestaña de repo; los repos tienen **alias
corto** en la barra superior; y el **gate de CI** distingue «sin runner» de un fallo real.

### Añadido
- **Alias de repositorios**: `repoAliases` en la configuración (`ruta → alias`), máximo 24 caracteres
  y solo letras, números, espacio y `. - _ /`; se edita desde el menú contextual de la pestaña y desde
  Ajustes › General, el tooltip conserva la ruta completa y sin alias se recorta el nombre.
- **Diff a dos columnas**: «Ajustar al ancho» (persistido), cabecera del hunk fija y navegación al
  cambio anterior/siguiente.
- **Cambios en árbol**: cada nivel de la ruta es un nodo propio, plegable y con el recuento de ficheros
  de su subárbol; el orden de ficheros es el mismo que en modo lista.
- **Etiquetas en el riel**: grupo propio con contador, dentro del filtro y con el nombre completo en el
  tooltip.
- **Tope de contexto de la IA**: ajuste «Tamaño de contexto» en Ajustes › IA (4 000–4 000 000 bytes);
  el aviso del informe de cambios apunta a ese ajuste y ofrece reintentar con más sin cerrar el diálogo.

### Corregido
- **Historial completo**: se construía con `--branches --remotes --tags HEAD`, que no alcanza al `HEAD`
  de otro worktree en «detached HEAD» ni a `refs/pull/*`. Ahora se usan todas las referencias de
  `refs/` (menos `stash`, `notes` y `bisect`) más el `HEAD` de cada worktree; hay prueba que compara lo
  listable con `git for-each-ref` sobre heads, remotes, tags y pull.
- **Refresco**: al completar una PR se refrescan estado, historial y lista de PRs (y si el merge falla
  no se refresca nada a medias); al activar otra pestaña de repo se recargan sus datos, sin duplicar
  peticiones si vuelves enseguida.
- **Gate de CI**: `cancelled`/`abandoned` (job sin runner) ya no se cuenta como fallo de tests; el gate
  reintenta el job y, si sigue sin runner, lo dice con su propio mensaje. `main` exige PR con checks.

## [0.1.0-alpha.11] - 2026-11-19

Entra lo que llegó a `main` después de la alpha.10.

**Respecto a `v0.1.0-alpha.10`:** el **historial** con «Grafo completo» recorre de verdad todas las
ramas y etiquetas (el ámbito por defecto ya no se degradaba a «solo HEAD»), las **pestañas de repos**
tienen separación real y recortan los nombres largos con puntos suspensivos, y el **riel de ramas**
muestra solo el segmento de cada entrada (el nombre completo sigue en el tooltip y el `aria-label`).

### Corregido
- **Historial**: `log_page` tomaba el ámbito ausente como «solo HEAD», así que el grafo completo podía
  quedarse en la rama actual y no aparecía un commit que vivía en otra rama. Ahora el defecto es el
  grafo completo (`--branches --remotes --tags HEAD`, en orden topológico) y solo `current` recorta a
  HEAD; la interfaz manda siempre el ámbito, también al paginar, y un repo sin commits devuelve lista
  vacía en vez de fallar.
- **Pestañas de la barra superior**: el espaciado era interno y el nombre largo desbordaba su caja y
  se pintaba encima de la pestaña vecina. Ahora hay `gap` real entre pestañas, la pestaña recorta su
  contenido y el nombre se acorta con puntos suspensivos.
- **Riel de ramas**: cada fila repite la ruta completa dentro de su carpeta (`origin/feature/rama`
  bajo `origin/` → `feature/`). Ahora muestra solo su segmento («rama»), con el nombre completo en
  el tooltip y el `aria-label`; la búsqueda, el menú contextual y las acciones siguen usando la
  referencia completa.

## [0.1.0-alpha.10] - 2026-11-19

Entra lo que llegó después de la alpha.9: los arreglos de la revisión de código.

**Respecto a `v0.1.0-alpha.9`:** los **conflictos de una PR** ya se leen de verdad (el código de
salida 1 de `merge-tree` dejaba la lista siempre vacía), un fichero recortado o binario no se puede
«resolver» con IA, el fetch baja `refs/pull/N/head` (PRs de forks), la rama local **no se fuerza**
nunca, el modelo de IA rechaza nombres que empiecen por `-` y el informe de cambios filtra lo
sensible. Además, actualizar una rama sin cambiarte a ella ya no da un error falso cuando está al
día.

### Corregido
- **Conflictos de PR con IA**: `git merge-tree` sale con 1 cuando hay conflictos y se tomaba por
  «sin conflictos», así que la lista de ficheros salía **siempre vacía** y los mensajes de git se
  colaban como rutas. Ahora se interpreta el código de salida (0 limpio, 1 conflictos, otro error)
  y la salida se lee con `-z --no-messages` (hace falta git 2.38 o posterior: se avisa si es más
  antiguo). Con prueba de integración en un repo temporal con un conflicto real.
- **Conflictos de PR**: un fichero cuya versión se recortaba a 60 KB se podía «resolver» con IA y
  se escribía el texto corto encima del real. Ahora se marca `truncated`, no se ofrece la IA y
  `pr_apply_conflicts` lo rechaza; además solo acepta rutas que estén **de verdad** en conflicto y
  calcula `merge_ready` mirando las rutas sin resolver del índice.
- **Conflictos de PR**: el `fetch` no bajaba la PR (ignoraba `refs/pull/N/head`, así que una PR de
  un fork daba «no encuentro la rama»). Ahora se bajan explícitamente la cabeza y la base a
  `refs/ariadna/pr/N/…`, se cachea el resultado por PR y SHA (antes cada fichero repetía fetch,
  merge-tree y tres `git show`), el consentimiento enseña el fichero que se manda y se aplican las
  reglas de exclusión (también `.env` y rutas sensibles).
- **Preparar el merge de una PR**: `checkout_remote_ref(…, true)` **rebasaba la rama local** a la
  cabeza de la PR y podía pisar commits locales. Ahora no se fuerza: si diverge, se para y se avisa.
- **Actualizar una rama no activa**: si ya estaba al día devolvía «no es avance rápido» (un error
  falso). Además el refspec usaba el nombre local en vez del **upstream real**
  (`branch.<nombre>.merge`), y una rama activa en otro worktree ahora se explica como tal.
- **Modelo de IA**: un nombre que empezara por `-` se aceptaba, así que
  `--dangerously-skip-permissions` pasaba como modelo y el CLI lo leía como una opción. Se rechaza,
  se valida también al cargar los ajustes y se guarda recortado.
- **Informe de cambios**: el diff se mandaba sin las exclusiones de la IA (un `.env` iba al CLI),
  de un merge solía salir vacío y se dejaba ejecutar `textconv` del repo. Ahora filtra lo sensible,
  usa `-m --first-parent --no-textconv` y el consentimiento dice cuántos ficheros se excluyen.
- **Guardar el informe**: `create_new` en una sola operación (antes se comprobaba y escribía en dos
  pasos), se rechaza `informes-cambios` como enlace simbólico y los nombres reservados de Windows
  (`CON.md`, `NUL.md`…).
- **Historial**: el grafo completo usa `--branches --remotes --tags HEAD` en vez de `--all`, así no
  mete el stash, las notas ni `refs/pull/*`; los métodos del trait para el historial completo ya no
  tienen implementación por defecto (un motor no puede fingir que los soporta).

## [0.1.0-alpha.9] - 2026-11-18

Entra lo que llegó después de la alpha.8.

**Respecto a `v0.1.0-alpha.8`:** el **historial muestra el grafo completo** (todas las ramas
locales y remotas y las etiquetas, en orden topológico) con un filtro para acotar a la rama actual,
y las **pestañas de repos** de la barra superior dejan de verse achatadas y pegadas.

### Añadido
- **Historial con el grafo completo**: la vista por defecto recorre **todas las ramas (locales y remotas) y las etiquetas** en orden topológico (`git log --all --topo-order`), no solo la rama actual, para ver de un vistazo dónde se bifurca y dónde se fusiona cada cosa. Dos botones en la cabecera del historial —**Grafo completo** / **Rama actual**— acotan el recorrido: en repos enormes, «Rama actual» vuelve al coste de antes (y por cursor). La búsqueda sigue al ámbito elegido, y el carril del grafo admite ahora hasta 12 columnas antes de aplastar las extra.

### Corregido
- **Pestañas de la barra superior**: tenían 30 px de alto pegadas al borde y 2 px entre ellas, así que se veían achatadas y se tocaban. Ahora hay 5 px de aire arriba y abajo dentro de la tira, 6 px entre pestañas, 28 px de alto con radio de 9 px y 3 px de margen lateral para que no se recorten las esquinas ni el anillo de foco al desplazar.

## [0.1.0-alpha.8] - 2026-11-18

Entra lo que llegó después de la alpha.7.

**Respecto a `v0.1.0-alpha.7`:**
- Resolución de conflictos de una pull request **con IA** (leer los ficheros en conflicto, hasta tres
  alternativas y aplicar con confirmación, sin commit ni push).
- Fila **Modelo** de Ajustes › IA usable: ancho completo, desplegable correcto y campo «Otro…» editable.
- La tira de repos de la barra superior tiene **scroll horizontal propio** (rueda incluida) con barra
  visible: las pestañas de la derecha dejan de ser inalcanzables.
- El **scroll queda atrapado** dentro de Ajustes: se bloquea el `body` y solo se desplaza el panel de
  la sección, con la cabecera fija.

### Añadido
- **Resolver los conflictos de una pull request con IA**: cuando la PR tiene conflictos, su detalle muestra **«Resolver conflictos con IA…»**. Ariadna baja las referencias, lee el merge que git calcularía (`git merge-tree`, sin tocar el árbol de trabajo) y enseña cada fichero en conflicto con sus tres versiones. Por fichero, la IA propone **hasta tres alternativas razonables** (con su motivo), se ven completas antes de elegir y nada se escribe hasta pulsar **«Aplicar y preparar merge»**, que pide confirmación nativa y deja el merge en local con los ficheros resueltos y preparados. El commit y el push los hace el usuario: la PR no se modifica sola. Errores claros para binarios y ficheros sensibles.

### Corregido
- **Ajustes › IA, fila Modelo**: el campo era inutilizable (heredaba el ancho de las casillas). Ahora la fila ocupa el ancho del panel, el desplegable abre bien y el campo de «Otro…» deja escribir, seleccionar y corregir, con foco visible y estado deshabilitado claro.
- **Barra superior con muchos repos**: las pestañas se amontonaban y las de la derecha no se podían alcanzar. La tira tiene ahora scroll horizontal propio (rueda y rueda horizontal incluidos) con barra fina visible, sin tocar el resto de la barra.
- **Scroll de Ajustes**: al abrir Ajustes se desplazaba la aplicación entera y el diálogo podía quedar a medias con media pantalla vacía. El scroll es solo del contenido de Ajustes (cabecera fija), con el fondo bloqueado y sin encadenar el desplazamiento.

## [0.1.0-alpha.7] - 2026-11-18

Entra todo lo que llegó después de la alpha.6 (publicada el 2026-11-17 a las 22:19), más lo que
ya venía en la alpha.4, la alpha.5 y la alpha.6 y no estaba en la alpha.3.

**Respecto a `v0.1.0-alpha.6`:**
- Se retira el experimento de SDD: fuera la carpeta `specs/` y todo lo que solo existía por él;
  la documentación viva que había dentro pasa a `docs/`.
- README reescrito en inglés y sin ninguna referencia a Swift, con el estado real verificado y la
  tabla de documentos de `docs/`; la descripción del repositorio en GitHub también se actualizó.
- Modelo de IA por proveedor en Ajustes › IA.
- Informe de cambios `.md` desde el menú contextual de un commit del historial.

### Añadido
- **Modelo de IA por proveedor**: Ajustes › IA permite elegir el **modelo**, no solo el proveedor. Ninguno de los tres CLI publica una lista de modelos (su `--help` solo documenta `--model`/`-m`), así que el desplegable trae los alias que cada CLI documenta (`sonnet`, `opus`, `haiku`, `fable` en Claude; `auto` en Copilot; ninguno en Codex) más **«Otro…»**, que abre un campo de texto con validación básica (letras, números y `- _ . : /`, máximo 64; si no, `INVALID_MODEL`). Se guarda en los ajustes y se aplica a todas las llamadas: `--model` en Claude y Copilot, `-m` en Codex. Vacío = el modelo por defecto del CLI.
- **Informe de cambios de un commit**: el menú contextual del historial trae **«Generar informe de cambios (.md)…»**, que pide a la IA un Markdown de ese commit con cuatro secciones: *qué cambia* a nivel funcional (no línea a línea), *riesgos y zonas sensibles*, *posibles bugs* y *qué probar*. Se pinta en el visor y solo se escribe en el repositorio (`informes-cambios/<sha>-informe.md`) cuando pulsas **«Guardar»**; nunca sobrescribe (`CONFLICT`) y también se puede copiar. El commit lo lee Rust (`git show`), el diff se recorta al tope de contexto (`truncated`) y valen los mismos filtros de siempre: archivos sensibles fuera, secretos redactados, consentimiento por perfil y proveedor.
- **Actualizar una rama sin cambiarte a ella**: el menú contextual del riel trae **«Actualizar desde «x» (sin cambiar de rama)»**, que trae esa rama local desde su remoto (`git fetch x rama:rama`). Solo avanza si es un avance rápido; si divergió, git lo rechaza y Ariadna lo explica con su propio mensaje, sin forzar nada. En la rama activa sigue estando «Bajar la rama actual», y el menú dice cuál toca en cada caso.
- **Pestañas de ancho ajustable**: arrastra el borde derecho de una pestaña para hacerla más ancha o más estrecha (112–420 px); el ancho se recuerda entre sesiones. Con el tirador enfocado, ←/→ ajustan de 12 en 12 y `Supr` (o doble clic) vuelve al ancho automático.
- **Árbol de ramas anidado y plegable en el riel**: las ramas se agrupan por **todos** los segmentos, no solo el primero, así que `origin/release/1.0/hotfix` cuelga de `origin/` → `release/` → `1.0/`, con contador en cada carpeta y sangría por nivel. Cada carpeta se abre y se cierra por separado (`aria-expanded`), y al escribir en el filtro se despliegan todas para ver las coincidencias.
- **Crear etiquetas con el botón derecho**: el menú contextual de una rama del riel trae **«Crear etiqueta…»**, que abre la ventana de nueva etiqueta apuntando al commit de esa rama. En el **historial**, el botón derecho (o `Mayús+F10`) sobre un commit abre un menú con **«Crear etiqueta…»**, «Copiar el SHA» y «Copiar el asunto». No se mueve nada del repositorio: la etiqueta se crea donde se ha pedido.
- **El diff se puede ver en dos columnas**: además de la vista de siempre (una columna), hay un selector **Una columna / Dos columnas** en la cabecera del diff y en el detalle de commits y stashes, con lo antiguo a la izquierda y lo nuevo a la derecha. Se recuerda entre sesiones (`ariadna.diffLayout.v1`). Las líneas iguales salen en las dos columnas y un bloque de borradas se empareja con el de añadidas. En las pull requests la vista partida no se ofrece: los comentarios se anclan a una línea concreta.

### Cambiado
- **La barra superior ya no muestra la rama**: era información duplicada (el riel de la izquierda la muestra siempre, con su seguimiento y sus acciones) y con muchas pestañas de repos abiertas el chip se solapaba con el resto. La barra queda con las pestañas, el buscador de la paleta y los botones de tema, densidad y ajustes. El atajo para saltar al buscador del riel sigue funcionando.

### Corregido
- **Botón de tema de la barra de título**: ahora alterna claro ↔ oscuro con dos estados e icono de la acción (luna/sol). Antes pasaba por «sistema» como tercer estado, donde el icono se convertía en un monitor y parecía que el botón había desaparecido. «Sistema» sigue en Ajustes › General y en la paleta de comandos. Con prueba E2E (seis pulsaciones comprobando que el botón sigue visible y con icono) y pruebas unitarias del ciclo.

### Verificado
- **Codex CLI**: verificado el 2026-11-17 con `codex-cli 0.169.0` ejecutando el CLI real: `codex exec --json --sandbox read-only --skip-git-repo-check -` con el prompt por stdin, JSONL (`item.completed`/`agent_message`) y `codex login status` («Logged in using ChatGPT»). **`CODEX_HOME` sí aísla la sesión** (directorio vacío → «Not logged in»), así que Codex deja de ser experimental y puede tener sesión propia en el perfil Empresa. Documentado en [`docs/ia-codex.md`](docs/ia-codex.md).

### Decidido
- **D16 (perfiles): la IA se permite directamente en el perfil Empresa** (Codex y Copilot), con un aviso permanente en Ajustes › IA de que esos proveedores comparten la sesión con el perfil Personal y de que el código del trabajo se enviaría a la cuenta personal del CLI. Se prioriza el uso real en el trabajo, ya que el aislamiento no es posible con esos CLI (solo Claude lo soporta y no está disponible en la empresa).

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
  commits y diffs grandes, y el informe `docs/rendimiento.md` con las medidas contra
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

[0.1.0-alpha.8]: https://github.com/jamartif/Ariadna/compare/v0.1.0-alpha.7...v0.1.0-alpha.8
[0.1.0-alpha.7]: https://github.com/jamartif/Ariadna/compare/v0.1.0-alpha.6...v0.1.0-alpha.7
[0.1.0-alpha.2]: https://github.com/jamartif/Ariadna/compare/v0.1.0-alpha.1...408b0fb
[0.1.0-alpha.1]: https://github.com/jamartif/Ariadna/commit/1ecdd35
