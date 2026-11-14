# Menú contextual del riel de ramas — capturas

Comprobación en navegador (`npm run dev` con el backend simulado, 1440×900) del menú que abre
**clic derecho** —o **Mayús+F10** / tecla **Menú**— sobre una fila del riel.

| Captura | Qué muestra |
|---|---|
| `01-menu-rama-actual.png` | Menú de la **rama actual**: «Cambiar», «Fusionar», «Rebasar», «Borrar» y «Ver el diff» aparecen deshabilitados (con el motivo en el tooltip) y «Bajar la rama actual» sí está disponible. |
| `02-comparar-con.png` | Segundo paso «Comparar «main» con…» en el mismo menú: «Volver» y la lista de ramas (la propia no aparece). |
| `03-diff-contra-otra.png` | Resultado: el detalle de la rama pinta `git diff actual...esta` (lista de archivos + visor de solo lectura). |
| `04-reposicionamiento.png` | Abierto con el puntero abajo a la derecha: se voltea y se pega al borde (no se sale de la ventana). También se recoloca al abrir el menú de otra fila sin cerrar el anterior. |
| `05-rama-remota.png` | Fila del remoto (`origin/main`): se deshabilitan «Fusionar», «Subir», «Bajar», «Renombrar» y «Borrar la rama», y se habilita «Borrar en el remoto «origin»». |

Lo cubierto por las pruebas automáticas: `ui/src/lib/components/BranchMenu.test.ts` (apertura con
ratón y teclado, navegación con flechas, Escape y foco devuelto a la fila, acciones deshabilitadas,
reposicionamiento, «Comparar con…», portapapeles, enlaces web y axe con el menú abierto),
`ui/src/lib/stores/branchesMenu.test.ts` (comandos nuevos del store) y
`src-tauri/src/core/tests/branch_menu.rs` (repos temporales reales: push con upstream, rebase con y
sin conflictos, borrado en el remoto con cancelación y diff entre ramas con binarios y renombrados).
