# E2E y regresión visual de Ariadna

Pruebas de extremo a extremo contra la **app empaquetada en depuración** (no contra el
build web ni mocks) usando [`tauri-driver`](https://crates.io/crates/tauri-driver) y
[WebdriverIO](https://webdriver.io/) con Mocha. Además, un script de **regresión visual**
que compara capturas con baselines por sistema operativo.

- 9 ficheros de flujo (13 pruebas) sobre un repositorio sintético que crea la propia
  suite con `git` real.
- 8 pantallas × 2 temas (claro/oscuro) = 16 baselines por sistema operativo.
- Sin red y sin tokens: solo `git` local y el binario compilado.

## Requisitos

| Pieza | Versión / paquete |
| --- | --- |
| Node | ≥ 22.12 (lo mismo que `ui/`) |
| Rust | el de `rust-toolchain.toml` |
| `tauri-driver` | `cargo install tauri-driver --locked` |
| Linux (WSL incluido) | `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, `xvfb`, el driver de WebKit |
| macOS | Xcode CLT; `tauri-driver` usa el driver de WKWebView |
| Windows | WebView2 + `msedgedriver.exe` en el `PATH` (lo descarga `tauri-driver` si falta) |

El paquete del driver de WebKit cambió de nombre entre versiones de Ubuntu:

```bash
# Ubuntu 24.04 (CI)
sudo apt-get install -y webkit2gtk-driver
# Ubuntu 25.10 / 26.04
sudo apt-get install -y webkitgtk-webdriver
```

Comprueba que existe `WebKitWebDriver` en el `PATH` (`which WebKitWebDriver`).

## Puesta en marcha

```bash
# 1. Interfaz compilada: sus archivos se incrustan en el binario
cd ui && npm ci && npm run build && cd ..

# 2. Binario de la app CON la interfaz empaquetada
cargo build -p ariadna-app --features tauri/custom-protocol

# 3. Dependencias de las pruebas
cd e2e && npm ci
```

`--features tauri/custom-protocol` es obligatorio: sin él, `tauri::is_dev()` es
verdadero y la ventana carga `devUrl` (`http://localhost:5173`), así que las pruebas
estarían hablando con el servidor de Vite en vez de con el binario. La suite lo
detecta y aborta con un mensaje que lo explica.

## Ejecutar

Linux / WSL necesita un servidor X; en CI y en local se usa `xvfb-run`:

```bash
cd e2e
xvfb-run -a -s "-screen 0 1440x900x24" npm test            # los 9 ficheros (13 pruebas)
xvfb-run -a -s "-screen 0 1440x900x24" npm test -- --grep "rama"
xvfb-run -a -s "-screen 0 1440x900x24" npm run visual      # comparar con baselines
xvfb-run -a -s "-screen 0 1440x900x24" npm run visual:update  # regenerar baselines
```

macOS y Windows no llevan `xvfb-run` delante:

```bash
cd e2e
npm test
npm run visual
npm run visual:update
```

Si la sesión gráfica ya existe (escritorio de WSLg, macOS, Windows) no hace falta Xvfb.

## Qué prueban los flujos

| Fichero | Flujo | Comprobación real |
| --- | --- | --- |
| `01-abrir-repositorio` | Abrir un repo por ruta | Pestaña con nombre y rama; recuento de commits de git |
| `02-ver-cambios` | Cambios y diff | Secciones «Preparados / Sin preparar / Sin seguimiento» (`data-section`) y diff con la línea añadida |
| `03-preparar-archivo` | Preparar | `git diff --cached` contiene solo lo esperado |
| `04-crear-commit` | Commit | `git log -1` con el asunto escrito y HEAD nuevo |
| `05-crear-rama` | Crear rama (riel) | `refs/heads` de git, la rama aparece en el riel y la actual no cambia |
| `06-cambiar-rama` | Filtro + cambio de rama (riel) | Aviso por cambios sin confirmar → checkout efectivo en git |
| `07-historial-paginado` | Historial | Paginación 100 → 200 → 240 y commit más antiguo visible |
| `08-ajustes-tema` | Ajustes (`Ctrl+,`) | Tema oscuro/claro en `<html data-theme>`, `localStorage` y persistencia tras guardar |
| `09-pestana-nueva` | Pestaña nueva | Alta y cierre por teclado (`Supr` sobre la pestaña), el repo sigue abierto |

El armazón es el del rediseño: **riel de ramas** a la izquierda, **vista central con
control segmentado** (Cambios · Historial · Stashes · Pull requests · Conflictos) y
**panel de detalle/diff** a la derecha. Los pasos compartidos viven en `lib/flows.mjs`:

| Ayuda | Qué hace |
| --- | --- |
| `showView('changes'\|'history'\|'stashes'\|'pullRequests'\|'conflicts')` | Cambia la vista desde el control segmentado |
| `ensureRailOpen()` | Abre el riel si está contraído (`Ctrl+B`; si el driver no entrega la tecla, pulsa el botón) |
| `filterBranches(texto)` | Escribe en el filtro instantáneo del riel |
| `selectBranch(nombre)` / `checkoutBranch(nombre)` | Selecciona en el riel (inspector) y cambia de rama, aceptando el aviso de cambios sin confirmar |
| `createBranch(nombre, { from })` | Crea la rama desde la cabecera del riel |

Los localizadores son **roles, `data-testid` y atributos de datos** (`data-tab-id`,
`data-tab-kind`, `data-brail-name`, `data-section`): el texto visible solo se usa para
datos que no se traducen (rutas, nombres de rama, asuntos de commit). Los `data-testid`
viven en los componentes de la interfaz (`BranchRail`, `Workspace`, `TabStrip`,
`RepoDialog`, `ChangesPanel`, `CommitBar`, `DiffView`, `Welcome`, `SettingsDialog`).

### Idioma de las pruebas

La app arranca en el idioma guardado en `settings.json` (Rust es la fuente de verdad y
la interfaz lo aplica al montar). En Linux la suite escribe ese archivo en el
`XDG_CONFIG_HOME` aislado (`<tmp>/app-state/config/ariadna/settings.json`) con
`language: "es"` y espera a que `<html lang>` coincida antes de la primera prueba, así
que las aserciones no dependen del idioma del sistema operativo del runner. Se puede
cambiar con `E2E_LOCALE` (`es` por defecto; `en` para probar el catálogo inglés).

Los datos son sintéticos y deterministas (autor, fechas de 2020, mismo contenido) y se
crean en el directorio temporal, que se borra al terminar. Cada prueba deja una captura
en `e2e/artifacts/screens/` (también las que fallan).

### Aislamiento del estado

En Linux la suite apunta `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_CACHE_HOME` y
`XDG_STATE_HOME` a un directorio temporal, así que los ajustes, recientes y pestañas
restauradas del usuario no interfieren. En macOS y Windows Tauri no permite redirigir
esos directorios con variables: la suite cierra las pestañas de otros repositorios al
empezar (`flows.normalizeStart`). Para baselines limpios conviene ejecutar en una
máquina o cuenta sin repositorios abiertos.

## Regresión visual

```bash
npm run visual          # compara; sale ≠ 0 si alguna pantalla difiere
npm run visual:update   # escribe e2e/baselines/<so>/*.png y manifest.json
```

- Pantallas: `bienvenida`, `cambios`, `diff`, `historial`, `riel`, `ajustes`, `paleta`,
  `commit-preparado`; en `light` y `dark` (el tema se fija con `data-theme`, la misma vía
  que usa la app). `riel` captura el riel de ramas con la rama actual, el árbol y el
  inspector de la rama seleccionada (sustituye a la antigua pestaña «Ramas»).
- Cada tema arranca la app de cero y, al terminar, se deshace el `git add` del fixture,
  para que las capturas de claro y oscuro partan del mismo estado.
- Antes de cada captura se congelan transiciones y animaciones y se espera a que no haya
  operaciones en curso.
- Umbral configurable:

| Variable | Por defecto | Significado |
| --- | --- | --- |
| `VISUAL_PIXEL_THRESHOLD` | `0.18` | Diferencia por canal tolerada en un píxel (antialias) |
| `VISUAL_MAX_DIFF_RATIO` | `0.03` | Fracción de píxeles distintos tolerada (3 %) |
| `E2E_PLATFORM` | `process.platform` | Fuerza el directorio de baselines (`linux`, `darwin`, `win32`) |

Los baselines son **por sistema operativo** porque el renderizado de fuentes y las
métricas de ventana cambian: `e2e/baselines/linux/` se mantiene en CI; `darwin` y
`win32` se generan en local con `npm run visual:update` en cada máquina (paso manual,
documentado también en el workflow).

Incluso dentro de Linux la **versión de WebKitGTK** cambia el suavizado de fuentes:
las capturas hechas con WSL (2.5x) no coinciden píxel a píxel con las del runner
(ubuntu-24.04, 2.44). Por eso los baselines de `linux` se regeneran **en el runner**:

1. Actions → *E2E (tauri-driver)* → **Run workflow**, con `update_baselines` marcado.
2. El trabajo compila igual que siempre, se salta la comparación y ejecuta
   `npm run visual:update` sobre la imagen de CI.
3. Descarga el artefacto **`visual-baselines-linux`** (contiene los 16 PNG y
   `manifest.json`) y descomprímelo sobre `e2e/baselines/linux/`.
4. Revisa un par de capturas, commitea y vuelve a lanzar el workflow **sin** la marca:
   ahora la comparación usa los baselines de la misma imagen.

En local (WSL/macOS/Windows) se puede usar `npm run visual:update` para trabajar, pero
el resultado solo es válido para ese SO y esa versión de WebKit.

Los diffs se escriben en `e2e/artifacts/visual/<so>/<tema>-<pantalla>.diff.png`.

## Variables de entorno

| Variable | Por defecto | Uso |
| --- | --- | --- |
| `ARIADNA_BINARY` | `src-tauri/target/debug/ariadna-app` | Binario que arranca el driver |
| `TAURI_DRIVER_BIN` | `~/.cargo/bin/tauri-driver` o `PATH` | Driver |
| `TAURI_DRIVER_NATIVE` | autodetectado en Linux | Ruta a `WebKitWebDriver` |
| `E2E_PORT` / `E2E_HOST` | `4444` / `127.0.0.1` | WebDriver |
| `E2E_TMP` | `$TMPDIR/ariadna-e2e` | Fixtures y estado aislado |
| `E2E_TEST_TIMEOUT` | `120000` | Timeout por prueba (ms) |
| `E2E_WDIO_LOG` | `error` | Nivel de log de WebdriverIO (`info`, `debug`) |
| `VISUAL_PIXEL_THRESHOLD`, `VISUAL_MAX_DIFF_RATIO`, `E2E_PLATFORM` | ver arriba | Regresión visual |

## Artefactos

```
e2e/artifacts/
  tauri-driver.log          # salida del driver
  screens/ok-*.png          # captura por prueba
  screens/fallo-*.png       # captura en el momento del fallo
  visual/<so>/actual-*.png  # captura comparada
  visual/<so>/*.diff.png    # diferencias contra el baseline
```

## Problemas conocidos

- **«element click intercepted»**: pasa con la pestaña «nueva» y con campos tapados
  por tostadas; por eso los helpers pulsan desde la página (`clickSelector`, `fill`)
  en lugar de usar el clic del driver.
- **El panel de diferencias se queda en «Cargando diff»**: si se prepara el archivo
  que estaba seleccionado, la petición de diff anterior queda colgada. La suite de
  visual arranca de nuevo la app en cada tema y pide el diff de un archivo ya
  preparado antes de capturar; conviene revisarlo en la interfaz.
- **La ventana no llega a estar lista**: revisa `artifacts/tauri-driver.log` y
  ejecuta `npm run doctor`; casi siempre falta `xvfb-run`, `DISPLAY` o el driver de
  WebKit. `node scripts/diagnose.mjs` vuelca título, URL y captura de la ventana.
- **Captura distinta en CI y en local**: normal si cambian fuentes o la versión de
  WebKitGTK; regenera los baselines con `--update` en la misma imagen que CI.
- **`tauri-driver` no aparece**: `cargo install tauri-driver --locked` instala en
  `~/.cargo/bin`; comprueba que está en el `PATH`.
