# Interfaz bilingüe: capturas

Revision visual de la interfaz **en inglés** (catálogos `en*`), con el backend simulado
(`npm run dev` en `ui/`, `?scenario=` por defecto). Se cambiaron el idioma y el ancho a mano:

1. Ajustes › General › **Idioma → English** (se aplica al momento, sin reiniciar).
2. Ancho de ventana 1440 px y 1000 px.

| Captura | Qué comprueba |
| --- | --- |
| `workspace-en-1440.png` | Vista de trabajo completa a 1440 px: pestañas de repositorio, riel de ramas, panel de cambios, formulario de commit (`Create commit Ctrl+Enter`) y diff. |
| `workspace-en-1000.png` | La misma vista a 1000 px: botones y etiquetas siguen enteros, sin cortes. |
| `settings-en-1440.png` | Ajustes › General: `Language` con las tres opciones (System / Español / English), tema y tamaño del diff. |
| `settings-en-1000.png` | Ajustes a 1000 px: la columna de secciones y los campos no se aprietan. |
| `settings-ai-en-1440.png` | Ajustes › Artificial intelligence: textos largos y perfiles traducidos (`Personal` / `Company`). |
| `history-en-1440.png` | Historial y detalle de commit: fechas en inglés (`November 6, 2026 at 10:00 AM`) y tiempos relativos (`7 days ago`). |

Queda en español a propósito: el contenido del repositorio (mensajes de commit, diff, nombres de
rama y de pestaña) y los mensajes que emite git o Rust (p. ej. la ruta de git no válida o el
aviso de `settings.json.bak`).
