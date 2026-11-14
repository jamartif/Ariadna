# Menú contextual del riel de ramas

Cada fila del riel (rama local o del remoto) tiene un menú con sus acciones. Se abre con
**clic derecho** sobre la fila o con la tecla **Menú** / **Mayús+F10** con la fila enfocada
(la fila se enfoca con `Tab` o con ↑/↓ dentro del riel). El menú se coloca en el punto del
puntero (o junto a la fila, si se abrió con el teclado) y **se voltea y pega al borde** si no
cabe en la ventana.

Las acciones que no aplican a esa rama **siguen visibles pero deshabilitadas**, con el motivo
en el globo de ayuda y un icono de prohibido. Se pueden enfocar para leer el motivo; pulsarlas
no hace nada.

## Teclas

| Tecla | Qué hace |
|---|---|
| `Mayús+F10` / tecla Menú | Abre el menú de la fila enfocada |
| ↑ / ↓ | Mueve el foco entre acciones (con vuelta) |
| Inicio / Fin | Primera / última acción |
| → | Entra en «Comparar con…» (segundo paso) |
| ← | Vuelve del segundo paso a las acciones |
| `Intro` / `Espacio` | Ejecuta la acción enfocada |
| `Esc` | Cierra el menú y **devuelve el foco a la fila** |
| `Tab` | Cierra el menú y sigue el recorrido normal del foco |
| Clic fuera / cambiar de ventana | Cierra el menú |

## Acciones

«Actual» = la rama que está en `HEAD`. «Remota» = una fila tipo `origin/rama`.

| Grupo | Acción | Qué hace | Atajos | Requiere |
|---|---|---|---|---|
| Cambiar | **Cambiar a esta rama** | `git switch` a esa rama. En una rama del remoto crea antes la local (`rama`) con su upstream. Si hay cambios sin confirmar, el riel pide confirmación y ofrece «cambiar igualmente» | Doble clic en la fila (sin abrir el menú) | No ser la actual; que no haya otra operación en curso |
| | **Crear una rama desde esta…** | Abre el formulario de nueva rama del riel con el punto de partida ya relleno | — | Que no haya otra operación en curso |
| | **Fusionar esta rama en la actual** | `git merge` de esa rama sobre la actual; confirmación nativa y conflictos al panel de Conflictos | — | Ramas locales (no remotas); que no sea la actual; que no haya merge/rebase en curso |
| | **Rebasar la actual sobre esta** | `git rebase` de la actual sobre esa rama; confirmación nativa | — | Que no sea la actual; que no haya merge/rebase en curso |
| Red | **Traer del remoto «x»** | `git fetch` del remoto de esa rama (el suyo si es remota; si no, el de su upstream u `origin`) | — | Que la rama tenga remoto; que no haya otra operación en curso |
| | **Subir la rama** | `git push`; con `--set-upstream` si la rama no lo tiene | — | Rama local con remoto |
| | **Bajar la rama actual** | `git pull` del remoto de la rama actual | — | Ser la rama actual; que tenga remoto |
| Escribir | **Renombrar…** | Abre el formulario de renombrado del inspector con el nombre actual | — | Rama local |
| | **Borrar la rama** | `git branch -d/-D` con confirmación nativa (segunda confirmación si no está fusionada) | — | Rama local y no actual |
| | **Borrar en el remoto «x»** | Borra la rama **en el servidor**; confirmación nativa | — | Rama del remoto |
| Copiar / comparar | **Copiar el nombre** / **Copiar el SHA** | Portapapeles del sistema; aviso con el valor copiado. Si el portapapeles falla, aviso de error | — | Nada |
| | **Ver el diff contra «actual»** | Pinta en el panel derecho `git diff actual...esta` (lista de archivos + visor de solo lectura) | — | Que la rama no sea la actual |
| | **Comparar con…** | Segundo paso **en el mismo menú**: «Volver» y la lista de ramas (la propia no aparece). Al elegir una, hace el mismo diff `A...B` | → para entrar, ← para volver | Que exista al menos otra rama |
| Web | **Abrir en el navegador** | Abre la página de la rama en GitHub / Azure DevOps / GitLab / Bitbucket. La URL la construye **Rust** desde la configuración real del remoto (solo `https`, host del propio remoto) | — | Que el remoto tenga página web y esté reconocido |
| | **Crear pull request** | Abre la página de nueva PR/comparación del proveedor (es un enlace; la API de PR vive en el panel de pull requests) | — | Que el proveedor del remoto admita PR (Azure DevOps, GitHub y GHE) |

Detalles que conviene saber:

- **Nada destructivo ocurre dentro del menú**: borrar rama, borrar en el remoto, fusionar y
  rebasar abren el **diálogo nativo de confirmación** de Rust. El menú solo lanza el comando.
- Con una operación en curso en la pestaña (o en la cola de remotos) las acciones que mutan
  quedan deshabilitadas con «Hay una operación en curso».
- Con un merge, rebase o cherry-pick a medias, fusionar y rebasar quedan deshabilitadas
  («Hay un merge o rebase en curso»): primero se termina o se aborta en el panel de
  Conflictos.
- En **HEAD desacoplado** fusionar, rebasar y «Ver el diff contra la actual» se deshabilitan
  con «No hay rama actual».
- El diff se compara **desde la base común** (`A...B`), así que enseña lo que la rama
  aportaría al fusionarla, no lo que le falta respecto a la actual.

## Contrato y pruebas

- Contrato IPC: [`specs/rust/ipc.md`](../specs/rust/ipc.md) «Menú contextual del riel de ramas».
- Componente: `ui/src/lib/components/BranchMenu.svelte` (lo abre `BranchRail.svelte`).
- Pruebas: `ui/src/lib/components/BranchMenu.test.ts` (ratón y teclado, foco, acciones
  deshabilitadas, reposicionamiento, segundo paso, portapapeles, enlaces y axe),
  `ui/src/lib/stores/branchesMenu.test.ts` y `src-tauri/src/core/tests/branch_menu.rs`
  (repositorios temporales reales).
- E2E: `e2e/test/10-menu-contextual-rama.e2e.mjs` sobre la app empaquetada (clic derecho,
  `Mayús+F10`, `Esc` con el foco de vuelta, acciones deshabilitadas, «Copiar el nombre» y el
  diff en el panel derecho).
- Capturas de la revisión visual: [`specs/rust/contextmenu/`](../specs/rust/contextmenu/README.md).
