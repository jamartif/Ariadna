# Rendimiento contra NF-1…NF-6

Medidas reales del motor Git (`ariadna-git`) con repos sintéticos. Cubren **lo que se puede
medir sin ventana**: `git` + parsers + serialización. NF-1, NF-2, NF-6 y la parte de
interfaz de NF-3/NF-4/NF-5 necesitan una ventana real y **siguen sin medirse** (ver el final).

## Cómo reproducir

```text
wsl.exe -d Ubuntu -u root -- env ARIADNA_TARGET=target-perf bash /mnt/c/Users/javie/dev/wcargo.sh \
  test -p ariadna-git --release --test perf -- --ignored --nocapture --test-threads=1
```

Las pruebas (`crates/ariadna-git/tests/perf.rs`) son `#[ignore]`: `cargo test` normal no las
ejecuta. Variables: `ARIADNA_PERF_FILES` (50 000), `ARIADNA_PERF_COMMITS` (10 000) y
`ARIADNA_PERF_COMMIT_GRAPH=1`. Cada medida sale como una línea `PERF <nombre> <valor>`.
Mediana de 5 ejecuciones salvo donde se indica «una vez».

## Máquina

WSL2 (Ubuntu, kernel 6.18), AMD Ryzen 7 7730U, 16 hilos, 19 GB, git 2.53.0, build
`--release`. Los repos viven en `/tmp`, que en esta WSL es **tmpfs (RAM)**: la E/S es más
rápida que en un disco real, NTFS o APFS. Con otros procesos compilando en paralelo las
medidas subieron de 1,5 a 4 veces; aquí van las de la ejecución más tranquila. Windows y
macOS no se han medido (los antivirus y NTFS suelen encarecer `status` de forma notable).

## Resultados

### NF-3 — `status` completo con 50 000 archivos (umbral ≤ 2 s)

| Medida | Resultado | Veredicto |
|---|---|---|
| `status` con 50 000 archivos sin seguimiento (`-uall`) | 66 ms | cumple |
| `status` con 50 000 archivos modificados (rastreados) | 412 ms (primera: 445 ms) | cumple |
| `status` con 50 000 archivos preparados | 163 ms | cumple |
| `commit` de 50 000 archivos | 1,4 s | — |
| `diff` de árbol completo, 50 000 archivos (una vez) | 2,7 s | — |

El umbral de NF-3 se afirma con `assert!` en las pruebas (la medida incluye lanzar `git`,
parsear 50 000 registros y devolver `GitStatus`). «Sin bloquear la UI» no se mide aquí: el
motor corre en tareas de tokio y la cola serial no ocupa el hilo de la interfaz, pero falta
comprobarlo con ventana.

### Rutas por stdin (50 000 rutas, 2,2 MB)

El cuerpo NUL-separado de 50 000 rutas ocupa **2 238 890 bytes**: no cabe en `argv` en
Linux (`E2BIG`, ≈ 2 MiB) y mucho menos en Windows (32 767 caracteres). Por stdin sí.

| Operación | Antes | Ahora |
|---|---|---|
| `stage` de 50 000 rutas | 35,5 s | 6,9 s |
| `stage` de 50 000 modificados | 49,1 s | 5,8 s |
| `unstage` de 50 000 rutas | 34,3 s | 0,19 s |
| `diff` con 50 000 rutas | error `E2BIG` (mostrado como «no encuentro git») | 2,7 s |

**Cuello de botella corregido.** `git add/restore --pathspec-from-file` compara cada archivo
del índice con cada pathspec: coste cuadrático (50 000 × 50 000). Trocear la lista no ayuda
(el producto total es el mismo). Por encima de 2 000 rutas (`BULK_PATHS` en
`engine/core_ops.rs`) el motor usa órdenes de índice lineales:

* `stage` → `git update-index --add --remove -z --stdin`.
* `unstage` → `git ls-tree -r -z HEAD` + `git update-index -z --index-info` (las rutas que
  HEAD no tiene se quitan del índice con modo 0).
* `diff` con rutas → diff completo y filtro en memoria (git diff no lee rutas de stdin).

Diferencias de semántica (solo en la vía masiva): las rutas se tratan como **archivos
literales**, así que una carpeta no se expande y `stage` no respeta `.gitignore` para una
ruta ignorada pedida explícitamente. La interfaz solo envía rutas de entradas de `status`,
por lo que no se observa. Cubierto por `tests/bulk_paths.rs` (2 600 rutas con espacios,
`-`, `ñ`, `[glob]*?`, borrados y archivos nuevos que HEAD no tiene).

Siguen siendo cuadráticos (no cambiados, destructivos o poco usuales): `restore_worktree` con
decenas de miles de rutas y `stage_hunk`/`unstage_hunk` (un hunk, sin impacto).
Además, `diff` de archivos **sin seguimiento** lanza un `git diff --no-index` por archivo:
con miles de ellos seleccionados a la vez sería lento (no medido).

### NF-4 — historial paginado por 200

Repo generado con `git fast-import` (un archivo tocado por commit).

| Repo | Página 200 en `skip` 0 | 25 % | 50 % | 75 % | final |
|---|---|---|---|---|---|
| 10 000 commits | 10 ms | — | — | — | 38 ms (`skip` 9 800) |
| 100 000 commits, sin commit-graph | 13 ms | 200 ms | 403 ms | 393 ms | 481 ms |
| 100 000 commits, con commit-graph | 12 ms | 22 ms | 38 ms | 55 ms | 71 ms |

* Recorrido completo de 10 000 commits en 51 páginas: 1,2 s; página más lenta 43 ms;
  sin repetidos ni huecos (comprobado). En 100 000 sin commit-graph el recorrido completo
  llegó a 168 s (cuadrático: `git log --skip` recorre todos los saltados), por eso la
  prueba solo recorre entero hasta 20 000 commits.
* `search_log` (texto, y autor, límite 500) en 100 000 commits: 1,0–1,6 s sin commit-graph,
  0,5–1,0 s con él; en 10 000 commits ≈ 0,1 s.
* **Límite encontrado (resuelto con cursor):** con `--skip` el coste de una página crece
  linealmente con la profundidad (481 ms al final de 100 000 commits sin commit-graph).
  `log_page` acepta ahora `cursor` (la *frontera* del historial ya cargado, ver `ipc.md`
  «Historial por cursor»): `git log -n 200 <frontera> --`, sin `--skip`. Medido (release,
  `ARIADNA_PERF_COMMITS=100000`, página de 200):

  | Profundidad | `skip` sin commit-graph | **cursor** sin commit-graph | `skip` con commit-graph | **cursor** con commit-graph |
  |---|---|---|---|---|
  | 0 % | 9 ms | 7 ms | 10 ms | 8 ms |
  | 25 % | 95 ms | 7 ms | 19 ms | 7 ms |
  | 50 % | 166 ms | 6 ms | 28 ms | 7 ms |
  | 75 % | 244 ms | 7 ms | 45 ms | 7 ms |
  | final | 325 ms | 6 ms | 53 ms | 8 ms |

  Con cursor la página más lenta de todo el recorrido fue 12–13 ms, con o sin commit-graph.
  (Las cifras de `skip` de esta máquina son menores que las de la tabla de arriba; la
  proporción es la que importa.)
* **Recomendación (no hecha, no automática):** con `skip` (búsquedas por `query`) un
  commit-graph sigue ayudando mucho. Escribirlo (`git commit-graph write --reachable`, o
  `git config fetch.writeCommitGraph true`) crea archivos en `.git` que el usuario no pidió;
  si se ofrece, que sea opt-in explícito y solo en repos grandes (> ~50 000 commits).
* Lo de «scroll a 60 fps» es de la interfaz y no se mide aquí.

### NF-5 — diff grande

| Caso | Resultado |
|---|---|
| Archivo de 200 000 líneas (2,6 MB) todo cambiado: 400 000 líneas de diff | 281 ms (3 archivos juntos: 350 ms) |
| Serializar ese `FileDiff` a JSON | 79 ms, **44 MB** de JSON |
| Binario de 5 MB | detectado (`is_binary`), sin hunks |
| Una sola línea de 2 MB | el motor la devuelve **entera** (2 000 000 bytes) |
| Diff de más de 64 MiB (1,4 M líneas) | error `IO` claro tras 0,9 s |

**Bug corregido:** el runner corta la salida a 64 MiB y marca `truncated`, pero el motor lo
ignoraba y devolvía `Ok` con el diff cortado a media línea. Ahora `GitCliEngine::read`
devuelve `GitError::Io` («la salida de git diff es demasiado grande… pide una parte más
pequeña») sin cambiar el contrato IPC (el código `IO` ya existía).

**Límite conocido:** el colapso a partir de 5 000 líneas / 1 MB y el truncado de líneas
largas son de la interfaz (`ui/src/lib/util/diff.ts`: `GIANT_LINES`, `GIANT_BYTES`). El
backend envía el diff completo por IPC: un diff de 400 000 líneas son 44 MB de JSON. Falta
pedir al motor una versión por demanda (cabecera + contadores primero, hunks después) si se
quiere acotar el IPC; no se ha hecho porque cambiaría el contrato.

## Propiedades de los parsers (M1.11)

`crates/ariadna-git/tests/parser_props.rs`, 13 propiedades de `proptest` × 512 casos:
sin pánico con bytes arbitrarios y UTF-8 inválido en `status`, `diff`, log, ramas, remotos,
conflictos, `unquote_c_style` y validadores de rutas/referencias; todo prefijo de un diff
válido se analiza; `parse → rebuild` reproduce el diff byte a byte; los contadores `@@`
cuadran con las líneas clasificadas; el parche de archivo entero se reanaliza igual; el
número de entradas de `status` coincide con los registros generados (y truncar nunca
inventa entradas); resolver todos los conflictos deja el archivo sin marcadores y con el
contenido esperado (LF/CRLF, con y sin base). **Pánicos encontrados: ninguno.**

## NF sin medir

| NF | Motivo |
|---|---|
| NF-1 arranque en frío ≤ 1,5 s | Necesita la app empaquetada y una ventana real en cada SO |
| NF-2 primer pintado de `status` ≤ 300 ms | Mide interfaz + IPC; el motor da 66–445 ms con 50 000 archivos, pero falta el pintado |
| NF-3 «sin bloquear la UI» | Comprobar con ventana y repos grandes |
| NF-4 «scroll a 60 fps» | Rendimiento de la lista virtualizada, solo en la interfaz |
| NF-5 colapsado y truncado de líneas largas | Lógica de interfaz (probada con tests de la UI, no con tiempos) |
| NF-6 20 pestañas | Memoria y manejadores de watcher: requieren la app en marcha |
| Windows / macOS | Solo se midió Linux (WSL, tmpfs) |
