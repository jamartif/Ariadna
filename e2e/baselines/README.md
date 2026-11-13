# Baselines de regresión visual

Una carpeta por sistema operativo: `linux/`, `darwin/`, `win32/`. El nombre de cada
archivo es `<tema>-<pantalla>.png` (por ejemplo `dark-historial.png`), 16 por
plataforma: 8 pantallas × claro/oscuro.

## Por qué por SO

El tamaño de la ventana, el escalado del sistema y sobre todo el renderizado de
fuentes cambian la imagen incluso con el mismo HTML y el mismo CSS. Comparar
contra el baseline de otra plataforma da diffs falsos.

| Carpeta | Quién la mantiene |
| --- | --- |
| `linux/` | CI (`ubuntu-24.04`) y quien ejecute `npm run visual:update` en Linux/WSL |
| `darwin/` | Se genera en un Mac: `npm run visual:update` (paso manual, no hay runner macOS en el workflow) |
| `win32/` | Se genera en Windows: `npm run visual:update` (paso manual) |

Mientras un sistema operativo no tenga su carpeta, `npm run visual` marca las
pantallas como «sin baseline» y termina con error a propósito: así nadie confunde
«no comparado» con «correcto».

## Cuándo actualizar

- Cambio visual intencionado (tokens, espaciados, iconos, nueva pantalla).
- Actualización de WebKitGTK / WebView2 / macOS que cambie el antialiasing.
- Nunca porque «falla y quiero que pase»: revisa antes el diff de
  `e2e/artifacts/visual/<so>/*.diff.png` y confirma que el cambio es el esperado.

`manifest.json` guarda la fecha de generación y los umbrales con los que se crearon
los baselines de esa carpeta.
