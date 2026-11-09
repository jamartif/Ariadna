# Ariadna · interfaz (Svelte 5 + TypeScript + Vite)

La interfaz es una **vista**: el estado vive en Rust (`specs/rust/ipc.md`). Esta carpeta no toca `src-tauri/` ni `crates/`.

## Comandos

```bash
npm ci            # instalar (versiones exactas, package-lock.json versionado)
npm run dev       # vite en 0.0.0.0:5173 (strictPort); sin Tauri usa el backend simulado
npm run check     # svelte-check, TypeScript estricto, falla con avisos
npm test          # vitest run
npm run build     # dist/ (fuentes autoalojadas, nada inline: compatible con CSP estricta)
```

En WSL sobre `/mnt/c` no llegan eventos de archivos: `ARIADNA_POLL=1 npm run dev`.
Escenarios del mock: `?scenario=empty|nogit|slow|error`.

## Estructura

- `src/lib/api/`: `types.ts` (espejo de `models.rs`), `api.ts` (interfaz `Api`), `tauriApi.ts`, `mockApi.ts`, `validate.ts` (validadores estrictos para el test de contrato) y `fixtures/` (los genera Rust; no editar).
- `src/lib/stores/`: `app.svelte.ts` (pestañas y recientes), `tab.svelte.ts` (una pestaña; descarta snapshots con revisión menor), `toasts`, `settings`, `commands`.
- `src/lib/shortcuts/registry.ts`: registro ÚNICO de atajos con ámbito; el test detecta conflictos.
- `src/lib/styles/tokens.css`: todos los tokens de diseño; `tokens.test.ts` calcula los contrastes.
- `src/lib/i18n/es.ts`: catálogo de textos; el test exige que toda clave usada exista.
- `src/lib/components/`: componentes (sin librerías de UI).
