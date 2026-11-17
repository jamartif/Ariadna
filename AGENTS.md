# Reglas de trabajo en este repositorio

Ariadna es una app de escritorio: núcleo Rust + UI Svelte en `ui/` + `src-tauri/`. Estas reglas mandan
sobre la costumbre de "verificarlo todo después de cada cambio". El objetivo es claro: **no gastar el
turno esperando**.

## Presupuesto de verificación

1. **En iteración, solo lo afectado.** Mientras se está cambiando algo, ejecutar únicamente los tests
   del fichero tocado:
   `npx vitest run src/lib/components/<Fichero>.test.ts --pool=forks`
   La suite completa (`npm test`) se ejecuta **una vez**, antes de mergear. Nunca en cada retoque.

2. **`npm run build` no va en el bucle interno.** `npm run check` (svelte-check) + el test del fichero
   ya detectan tipos y roturas. El build se lanza **una vez al final** del bloque de trabajo.

3. **Nada de bloquear el turno esperando CI.** Está prohibido el patrón
   `for i in $(seq 1 N); do gh api ...; sleep 30; done`.
   - Para esperar un workflow: `terminal(background=true, notify=True)` o `gh run watch <id> --interval 30`
     **mientras se sigue trabajando** en otra cosa.
   - Si no queda nada más que hacer, se avisa al usuario de que se está esperando CI en vez de dejar
     el turno colgado media hora.

4. **Baselines visuales: solo si el cambio es visual, y una sola vez por PR.** No regenerar baselines en
   cada push; si el diff no toca render, no hay baselines que actualizar.

5. **Escaneos del repo prohibidos.** `git fsck --unreachable` sobre todos los blobs está fuera de
   presupuesto. Para buscar contenido en el historial, acotar:
   `git rev-list --objects --all -- <ruta>` o `git log -S'<cadena>' -- <ruta>`.
   Para purgar un secreto, reescribir el commit concreto, no escanear el repo entero.

## Cómo trabajar un cambio

1. Leer el código relevante (grep/sed acotados, no volcar ficheros enteros).
2. Aplicar el cambio.
3. `npx vitest run <fichero de test afectado>` y `npm run check`.
4. Commit en la rama de trabajo.
5. Si hay CI: lanzarlo en background y seguir. Cuando esté listo, revisar y mergear.

## Límites

- Trabajar siempre dentro de este repositorio; no tocar nada fuera de él.
- No publicar releases ni subir versiones sin que el usuario lo pida explícitamente.
- Si una tarea requiere más de ~30 minutos de reloj, parar y decirlo con lo que queda por hacer, en vez
  de encadenar esperas.

## Comandos de referencia

```bash
# UI (desde ui/, en WSL: wsl.exe -d Ubuntu -u root -- bash -lc 'cd /mnt/c/Users/javie/dev/Ariadna/ui; ...')
npm run check                 # svelte-check, falla con warnings
npx vitest run <test> --pool=forks
npm test                      # suite completa (solo antes de mergear)
npm run build                 # una vez al final

# Núcleo
cargo build -p ariadna-app --features tauri/custom-protocol
cargo test -p <crate>
```
