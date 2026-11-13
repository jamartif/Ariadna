# Fixtures de salida de los CLI de IA

| Fichero | Proveedor | Estado |
|---|---|---|
| `claude_*.json` | Claude `--output-format json` | **Reconstruido a partir del formato documentado de `claude 2.1.x`, NO capturado**: en esta tarea no se ejecutan prompts reales. Sustituir por golden files reales en el humo manual de M6. El primero lleva una línea de aviso previa para probar la tolerancia a banners. |
| `codex_*.jsonl` | Codex `exec --json` | **NO VERIFICADO**: inventado razonablemente a partir del parser del Swift original (`thread.started`, `item.completed`, `turn.completed`, `turn.failed`). Codex no está instalado. |
| `copilot_*.txt` | Copilot `-s` | **NO VERIFICADO**: texto plano inventado. Copilot no está instalado. |

Cuando se verifiquen los CLI reales (spike M0), reemplazar estos ficheros por salida real
y actualizar esta tabla.
