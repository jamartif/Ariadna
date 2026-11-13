# Revisión speckfish del spec Rust — decisiones y cambios (revisión 3)

**Origen:** revisión formal con speckfish (Spec Kit + Elephant-Goldfish) sobre `specs/rust/spec.md`, ejecutada el **2026-11-13** con **siete subagentes** (zoom out, catálisis inventiva, comprensión, crítico con tres revisores —diseño, estructura, estilo— y zoom out intermedio). Informes originales en `C:/Users/javie/dev/spec-review/.speckfish/runs/20261113-184625/artifacts/review/`.

**Veredicto de la revisión: `meets_standard = false`.** 4 bloqueos, 23 hallazgos mayores y 35 menores. Este documento registra, para cada uno, la decisión tomada y dónde se aplicó. No se ha tocado código ni `tasks.md`.

**Criterio de resolución.** Se distingue lo que **hay** de lo que se **decide**:

- Lo que el código ya hace se describe como está (y se cita el crate o el módulo).
- Lo que se decide pero aún no está se marca en el spec como **decidido, no implementado** y lleva su entrada en `tasks.md` cuando se implemente.
- Lo que se aplaza se dice con la versión a la que va.

---

## 1. Bloqueos

| ID | Bloqueo (informe) | Decisión | Qué se cambió y dónde |
|---|---|---|---|
| **B1** | El alcance de v0.1 contradice «nada más en v0.1» (L11) al incluir clonado con credenciales, stash, perfiles y PR completas de dos proveedores; L39 seguía diciendo «fuera de v0.1» de cosas ya incluidas, y L378 ya preveía recortar | **Corregir el spec.** Se sustituye la regla «paridad y nada más» por una tabla de alcance real, con marca [P]/[N], versión en la que entra y estado. Se reconoce que v0.1 incluye tres novedades grandes y se conserva la regla de corte, ahora con prioridad explícita (se recorta una novedad, nunca una paridad) | §1.1 reescrita por completo; se elimina «Nada más en v0.1»; se añade §1.2 con hitos M0–M8 y alfas; D1 actualizada; riesgo de §12 reescrito |
| **B2** | El login del navegador del GCM devuelve un token OAuth/Entra y las PR envían Basic (Bearer diferido a v0.2): los usuarios de Azure DevOps reciben `AUTH` permanente sin saber por qué | **Corregir el spec y cerrar la decisión (D24).** Las PR se autentican **solo con PAT**; el token OAuth nunca se usa para la API. En Azure DevOps la interfaz lo dice antes de la primera llamada y traduce `401`/`203` a un `AUTH` accionable con botón para guardar el PAT. El clonado con navegador sigue siendo válido para git | §4.3.2 «Autenticación: solo PAT»; D24 nueva; D22 aclarada (Bearer existe, no se usa en v0.1); §4.3.1 separa los dos caminos de credenciales; D20 reescrita (el PAT es de un solo uso, no se hace `approve`) |
| **B3** | Los componentes de arquitectura (`SessionManager`, `RepositorySession`, `GitEngine`, `ProcessRunner`, `AiService`, `AiProvider`) aparecen en un diagrama y no se definen | **Corregir el spec.** Tabla de componentes con una frase de responsabilidad y ruta del código. Se corrige el nombre inventado: no existe `SessionManager`, el gestor es `AppCore`; `RepositorySession` vive en `ariadna-git`, no en `src-tauri` | §3.1 nueva (tabla de 10 componentes); diagrama de §3 actualizado con `AppCore`, `GitServing` y `PullRequestProvider` |
| **B4** | La spec depende de documentos externos (`revision-egm.md`, `ipc.md`, `trazabilidad.md`, `specs/spec.md` del Swift) y no es autocontenida | **Corregir el spec.** Todo lo externo se resume dentro y se enlaza con ruta exacta; se declara que ningún requisito depende de leer otro documento. Se añade glosario de siglas y jerga | §0.1 (glosario), §0.2 (tabla de documentos con ruta y qué se toma), §0.3 (marcas de estado); enlaces relativos corregidos en todo el documento |

---

## 2. Hallazgos mayores del crítico (23)

| # | Hallazgo | Decisión | Qué se cambió y dónde |
|---|---|---|---|
| 1 | Token `Confirmed` y carrera con `revision`: no se define qué cambios invalidan el token, caducidad ni comportamiento de la interfaz | **Corregir el spec** (decisión concreta, no «pendiente») | §6: caducidad de 120 s; cualquier cambio de `revision` invalida; la UI avisa y ofrece repetir, no reabre sola; el diálogo es modal por ventana, así que no hay carrera con la cola |
| 2 | Clonado cancelado o interrumpido no es atómico | **Corregir con decisión D23**: clonar en temporal hermano y renombrar; registro `pending-clones.json` y barrido al arrancar | §4.3.1 «Atomicidad (D23)», marcado **decidido, no implementado**; D23 en §2. Se reconoce lo que hay hoy (borrar lo creado y tolerar fallos de borrado) |
| 3 | Bloqueos de git: no se dice si el estado ligero usa `--no-optional-locks`; no hay criterio para distinguir `index.lock` propio de ajeno | **Corregir; el mecanismo base ya existe** | §4.1: lecturas con `--no-optional-locks` y `GIT_OPTIONAL_LOCKS=0` (**implementado** en `ariadna-git/src/runner.rs`); criterio de 4 pasos con pid, hora de inicio y mtime (10 min) y confirmación nativa para borrar; se marca qué está implementado y qué no |
| 4 | `git credential fill` por llamada: usuario no definido, dos cuentas, avisos del llavero, sin justificar la falta de caché | **Aceptar y justificar** (sin caché) + corregir el usuario explícito | §4.3.2: usuario del remoto o el guardado por host, validación con `current_user` antes de escribir; cuatro razones para no cachear y umbral para revisarlo (TTL ≤ 30 s si la medición lo justifica) |
| 5 | Cambio de perfil con IA en vuelo: consentimiento y entorno eran del perfil anterior | **Corregir** | §5.1.1: cambiar el perfil de un repo cancela las llamadas en vuelo y en cola de ese repo; RF-IA8 añade «o al cambiar de perfil» |
| 6 | Cola de IA global: dos llamadas de 120 s bloquean todo; sin prioridad ni cuota por repo; límite por minuto sin valor; bucle ante `AiQuota` | **Corregir con decisión D26** | RF-IA8: 2 simultáneas, **20 por minuto** y **máximo una por repositorio** (valores reales de `Limiter::DEFAULT_*`); RF-IA9: sin cambio de proveedor por cuota; `AI_QUOTA` se muestra con la espera |
| 7 | Variables de aislamiento heredadas (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`) pasaban al hijo; faltaban permisos y limpieza del directorio aislado | **Corregir** | §5.1.1: la variable se inyecta **después** de la lista blanca y gana sobre la heredada (**implementado** en `ariadna_ai::env::provider_env`, `extra` posterior); ruta `profiles/<perfil>/<proveedor>`, permisos `0700`, borrado con el perfil; RF-IA3 aclara que el valor es el inyectado |
| 8 | Pruebas de PR solo con `wiremock` y fixtures escritos a mano; sin paginación, sin límite de tasa, sin prueba de redirección a otro host | **Corregir** | §4.3.2 «Pruebas»: paginación de dos páginas, `Retry-After`, redirección a otro host que no se sigue, fixtures de GitHub derivados del spike real (72 comprobaciones) y de Azure DevOps marcados como no verificados |
| 9 | Diff de PR sin decisión de origen, sin paginación y con tope de respuesta sin valor | **Corregir** | §4.3.2 «Diff de la PR»: parches del proveedor (GitHub `patch`, Azure dos versiones), no `git diff` tras fetch; 8 MiB y 3 000 archivos; omitidos con motivo; NF-5 actualizada |
| 10 | Jerarquía rota en §4: §4.3 y §4.4 eran `##` dentro de `## 4. Git`; «Clonar y credenciales» sin número | **Corregir** | Ahora `### 4.1`, `### 4.2`, `### 4.3`, `#### 4.3.1`, `#### 4.3.2` y `### 4.4` no existe (PR pasó a 4.3.2). Verificado el árbol de encabezados |
| 11 | Referencias cruzadas imprecisas (`§5`, `ipc.md`, RF-IA12 sin enlace) | **Corregir** | §0.2 con rutas; todas las referencias a `ipc.md`, `spike-pr.md`, `trazabilidad.md` y al spec del Swift son enlaces; §5.2 y §4.3.2 citados con número exacto |
| 12 | Tabla de decisiones desordenada (D1…D12, D14, D18, D16, D19…) | **Corregir** | §2 ordenada D1…D26, con vocabulario de estado único (Cerrada / Propuesta / Sustituida) y notas de sustitución en la propia fila |
| 13 | Contradicciones de alcance y estado: G-16 `[N]` en tabla `[P]`; D17 propuesta y «elegida»; D14 frente a RF-IA11 y MSI | **Corregir** | Tabla §4 sin `[P]` en la cabecera y marca por fila (G-16, G-17 y G-18 como [N]); D17 solo **Propuesta**; RF-IA11 sale de v0.1 y deja de ser mitigación (§5.3, §12); MSI declarado posterior a v0.1 y coherente con D14 |
| 14 | Hitos sin definir: M0, M5, M6, M8 y `alpha.3` | **Corregir** | §1.2 define M0–M8 con contenido, criterio de salida y estado; D13 incluye `alpha.3` y `v0.1.0` |
| 15 | «v1», «v1.1» y «v0.1» para lo mismo | **Corregir** | Título y cuerpo hablan de **producto 0.1**; la numeración del documento es «revisión 1/2/3» y vive en §14; `v0.1.0` solo como etiqueta de versión |
| 16 | Párrafo de más de 200 palabras que mezcla hipótesis, resultados del spike y valores por defecto | **Corregir** | §5.1.1 dividido en «valores por defecto» y «aislamiento por perfil», con lo verificado, lo no verificado y lo decidido separados |
| 17 | Referencias a personas («Javier puede vetarlas», «el PM lo marca como riesgo») | **Corregir** | Sin nombres ni siglas de rol en todo el documento: se habla de dirección, equipo de producto y decisiones; el rastro queda en §14 y en este informe |
| 18 | Jerga sin definir: oráculo, regla de oro, coalescidos, hibernan, deltas por sección, spike, shims, golden files, fixtures, grano fino | **Corregir** | §0.1 define 26 términos, incluidos todos los citados |
| 19 | Siglas sin glosario: PROPUESTA, GCM, PAT, OV, GHE, M0/M5/M6/M8, RF-IA, NF, PR | **Corregir** | §0.1; «PROPUESTA» pasa a «Propuesta» y se define en §2 |
| 20 | Lenguaje subjetivo o coloquial («aspecto moderno», «uso diario», «con pinta de token», «diseñados en cada pantalla») | **Corregir** | §1 (objetivo), §4.3.1 («variables con forma de secreto»), §7 («estados resueltos») y §1.1 sin «uso diario» |
| 21 | Celdas de tabla sobrecargadas y frases confusas (nombre de usuario del PAT de Azure DevOps) | **Corregir** | §1.1 con cuatro columnas y celdas cortas; §4.3.1 dice «en Azure DevOps el nombre de usuario puede ser cualquier valor no vacío» |
| 22 | Listas de proveedores incoherentes entre §1, §4.3 y §4.3.1 | **Corregir** | Una sola lista en §4.3 (tabla de URL) y otra en §4.3.1 (reconocimiento de serie: ADO, GitHub, GHE, GitLab, Bitbucket, más `otro`); §1.1 y §4.3.2 remiten a ellas |
| 23 | «datos consultados al redactar; revisar antes de decidir» sin fecha ni enlace | **Corregir** | §10 da la fecha (noviembre de 2026) y enlace a `docs/firma-de-codigo.md`, donde vive el detalle y la fecha de cada precio |

---

## 3. Hallazgos mayores del zoom out y de la catálisis inventiva

Se listan aparte porque añaden matices que el crítico no cubrió; cuando duplican un hallazgo del crítico, se indica.

| ID | Hallazgo | Decisión | Dónde |
|---|---|---|---|
| Z1 (M1) | El perfil Empresa no cumple su objetivo: solo Claude tiene aislamiento verificado y está desactivado allí; Codex/Copilot comparten login | **Corregir con decisión D16**: en Empresa la **IA queda apagada por defecto** y solo se habilita Codex o Copilot aceptando un aviso explícito de sesión compartida. Se escribe la promesa honesta: el perfil controla qué se permite, no con qué cuenta | §5.1.1, D16; **decidido, no implementado** (hoy Empresa trae Codex y Copilot habilitados) |
| Z2 (M2) | Las mitigaciones de riesgos citan RF-IA11, que es posterior a v0.1, y D14 excluye el despliegue gestionado | **Corregir**: RF-IA11 sale de v0.1 y no se usa como mitigación; los riesgos se apoyan en mecanismos que existen (apagado por defecto en Empresa, consentimiento, cuenta visible) | §5.3 RF-IA11, §12 tabla de riesgos, §5.1 precedencia de los tres niveles de apagado |
| Z3 (M3) | Worktrees comparten clave por `--git-common-dir`: se puede confirmar o descartar sobre el árbol equivocado | **Corregir**: sesión y cola por **ruta de worktree**; el directorio común solo sirve para avisar de que es el mismo repo | §3.3 y §8, **decidido, no implementado**; riesgo añadido en §12 |
| Z4 (M4) | `Auth` se detecta por texto de git y, si no coincide, el fallo sale como `NETWORK` sin explicación | **Corregir**: se declara como segunda excepción documentada y, si el texto no coincide, el error lleva el `stderr` redactado a la vista; nunca un `AUTH` inventado | §4.1 (excepciones) y §4.3 (autenticación de git) |
| Z5 (M5) | Optimización prematura en pestañas: hibernación, vigilantes selectivos y `status` cada 60 s en todas las pestañas | **Corregir con lo que hay**: solo la pestaña activa tiene vigilante; sondeo de 2,5 s únicamente cuando el vigilante se degrada. Se elimina la promesa de sondeo periódico a 60 s | §8 (cifras reales: debounce 300 ms, espera máxima 2 s, sondeo 2,5 s); NF-6 sin cambios de cifra |
| Z6 (M6) | Pruebas de PR e IA con fixtures no validados | **Aceptar y corregir el origen**: los fixtures de GitHub salen del spike real y se revisan antes de versionarlos; los de Azure DevOps se declaran no verificados | §4.3.2 «Pruebas» |
| Z7 (M7) | Altitud: detalle de implementación en el spec (rutas listas, entorno duplicado, cifras de proceso) | **Aceptar parcialmente**: se conserva lo que es requisito verificable (listas cerradas de `PathSafety`, lista blanca de entorno) y se remite a `ipc.md` y `spike-pr.md` para el detalle. Revisión-speckfish deja constancia de que no se añade más detalle técnico | §0.2, §4.2, §4.3.1, §5.3 |
| I1 | Segmentación del alcance: núcleo [P] primero y hosting en tren posterior o con *feature flag* | **Aceptar el principio, no el mecanismo**: se adopta la regla de corte con prioridad, sin *feature flag* (el crate ya está separado y publicado en `alpha.3`) | §1.1 |
| I2 | Modo Auto: el Swift saltaba ante varios fallos; RF-IA9 solo salta si no está instalado; estaba marcado `[P] ampliado` | **Corregir**: se declara **[N]** y se documenta la diferencia de comportamiento; D8 y RF-IA9 alineados con el código (único salto: `NotInstalled`) | §1.1, D8, RF-IA9 |
| I3 | «No se parsean mensajes humanos» frente a la excepción `Auth` | **Corregir**: dos excepciones explícitas y acotadas (`dubious ownership`, `AUTH`), ambas con prueba y con caída a error genérico redactado | §4.1 |
| I4 | «Secretos sin guardar» frente a `GCM_INTERACTIVE=never` y el login por navegador | **Corregir con D20/D24**: el PAT es de un solo uso (askpass temporal, sin `approve`), guardar es una acción aparte y la API de PR no usa el token del navegador | §4.3.1, §4.3.2, D20, D24 |
| I5 | Aislamiento del perfil Empresa: `HOME` efímero frente a bloquear proveedores sin aislamiento | **Elegir D16** (bloquear por defecto, habilitar con aviso). No se usa `HOME` efímero: obliga a re-loguear y puede romper el llavero de macOS | §5.1.1 |

---

## 4. Hallazgos menores (35): plan por tipo

No se corrigieron uno a uno para no inflar el nivel de detalle del spec (hallazgo Z7). Se agrupan por tipo, con lo hecho y lo que queda.

| Tipo | Aprox. | Qué se hizo | Qué queda |
|---|---|---|---|
| **Nomenclatura y versiones** (v1/v1.1/v0.1, `PROPUESTA`/`Propuesta`, ARM64 repetido en §1/D7/§10) | 4 | «Producto 0.1» + «revisión del documento»; estado único en §2; ARM64 solo en §1 y D7 | Nada |
| **Estados y vocabulario de decisión** (`[P] ampliado`, Auto repetido en cuatro sitios, §13 omitía D19/D20) | 4 | Marca por fila en §4; Auto explicado una vez en §1.1 y referenciado; §13 enumera todas las propuestas | Nada |
| **Errores del contrato IPC** (`Auth`/`AUTH`, faltaban `NOT_FOUND`, `POLICY`, `RATE_LIMITED`, `INVALID_REFERENCE`, `PARSE`, `IO`) | 1 | §3.4 con la lista completa en mayúsculas y una sola grafía; `ApiError` con `code/message/detail/action` | Confirmar en `ipc.md` y en `src-tauri/src/core/error.rs` que los códigos coinciden (tarea de contrato) |
| **Cifras sin respaldo** (60 s × 20 pestañas, `58+8=66`, NF-8 con objetivo vacío, umbrales en ejecutores compartidos) | 4 | §8 con las cifras reales del vigilante; NF-8 con objetivo; nota de margen 2× en CI; el inventario de pruebas remite a `trazabilidad.md` | Nada |
| **Duplicaciones y notas históricas** («sin telemetría» tres veces, «36 + 8», spike de M0 dos veces) | 3 | Una sola mención por idea; se elimina la nota histórica | Nada |
| **Referencias internas imprecisas** (`op:progress` frente a `clone-progress`, L140 frente a L188, «checkout seguro ya existente») | 3 | §3.4 nombra los dos eventos y su ámbito; §4.1 con la regla única de tiempos; PR-05 remite a las reglas de checkout de §8 | Nada |
| **Lenguaje y estilo** (mezcla inglés/español, «→ se niega», «avisa de qué credencial hace falta», celdas largas, listas de entorno casi idénticas) | 10 | Traducido el texto (se conservan los identificadores del código); «se rechaza»; §4.3.1 explicita la credencial por proveedor; celdas cortas; una sola lista de entorno de git (§6) y una de IA (§5.3) | Repaso final de estilo con el corrector del repositorio |
| **Contenido que faltaba por escrito** (qué es un `AuthMethod`, `ai_accounts`/`ai_login`, `PathSafety`, `EnvPolicy::git_default`, `.gitleaks.toml`) | 6 | §0.1 y §3.1 los sitúan; §5.1.1 y §6 los explican en una frase | Nada |

---

## 5. Decisiones técnicas tomadas (resumen)

Las que un lector querrá encontrar juntas:

1. **Clonado atómico:** temporal hermano + renombrado, con registro y barrido al arrancar (D23).
2. **Estado ligero:** `--no-optional-locks` y `GIT_OPTIONAL_LOCKS=0` en todas las lecturas (implementado).
3. **`index.lock`:** reintento acotado, después pid + hora de inicio + mtime; borrado solo manual y con confirmación (parcialmente implementado).
4. **`git credential fill`:** usuario explícito (remoto, luego Ajustes, luego vacío), validación con `current_user`, **sin caché** y con umbral de revisión justificado.
5. **Aislamiento por perfil:** la variable del proveedor se inyecta después de la lista blanca y sobrescribe la heredada; directorio `0700` propio y borrado con el perfil.
6. **Perfil Empresa:** IA apagada por defecto; habilitar Codex o Copilot exige aceptar que comparten sesión.
7. **Fin de RF-IA11 en v0.1:** sale del alcance y de la tabla de mitigaciones; vuelve con el despliegue gestionado de v0.2.
8. **PR con PAT:** solo Basic con PAT; el OAuth del GCM no vale para la API y se explica en la interfaz; `Bearer` queda para v0.2 (D22, D24).
9. **Cola de IA:** 2 simultáneas, 20/minuto, 1 por repositorio; sin fallback por cuota (D26).
10. **E2E:** `tauri-driver` solo en Linux en CI; macOS y Windows, manual y con checklist.

---

## 6. Lo que queda para una segunda pasada de speckfish

1. **Verificación real de Azure DevOps** (bloquea la calidad de §4.3.2, no la estructura): `401` frente a `203` con token OAuth, permisos mínimos por operación, plantillas y work items, políticas de rama. Al verificarlo, cambiar el estado de «sin verificar» a «verificado» y actualizar §4.3.2 y `spike-pr.md`.
2. **Cerrar las marcas «decidido, no implementado»**: clonado atómico (D23), pasos 3 y 4 del criterio de `index.lock` (D25), Empresa con IA apagada (D16) y clave de sesión por worktree (§3.3). Cada una necesita su tarea y su prueba; hoy el spec las declara con honestidad.
3. **Alinear `ipc.md` con §3.4 y §4.3.2**: `ipc.md` aún dice «pendiente: UI del panel de PR» (ya existe en `ui/src/lib/components/PullRequestsPanel.svelte`) y hay que comprobar que los códigos de error coinciden con los del `ApiError` real.
4. **Regenerar los fixtures de PR** desde el spike real tras cada cambio de API y dejar escrito el script; hoy se citan 72 comprobaciones de GitHub, sin mecanismo de actualización.
5. **Medir la latencia de `git credential fill`** con el panel de PR abierto a 60 s (dos cuentas en el mismo host) antes de descartar definitivamente la caché con TTL.
6. **Repaso de estilo final** (mezcla de inglés y español en el texto, no en los identificadores) y de longitud de las celdas de tabla.
7. **Revisar que ninguna decisión «Propuesta» siga abierta al cerrar M7**: §13 debe quedar vacía o con las propuestas convertidas en cerradas.
8. **Riesgo residual declarado**: el perfil Empresa protege por política de producto, no por aislamiento técnico de Codex y Copilot; si la empresa exige garantía técnica, la única vía es desactivar la IA por completo o esperar a la política administrable de v0.2.
