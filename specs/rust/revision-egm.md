# Revisión EGM del spec Rust (borrador v0 → v1)

Método: tres revisores con contexto limpio (sin la conversación previa), cada uno con un rol. Solo vieron el borrador, el plan y las tareas, y el repo Swift cuando lo necesitaron. Los hallazgos son suyos; la columna «Resolución» es del PM.

Revisores: **R1** procesos, seguridad y multiplataforma (25 hallazgos) · **R2** arquitectura, UX y rendimiento (17) · **R3** alcance, testabilidad y plan (17).

Flags verificados por R1 contra `claude 2.1.319` y `git 2.53`: `-p`, `--output-format`, `--input-format`, `--tools ""`, `--disallowedTools`, `--permission-mode`, `--strict-mcp-config`, `--setting-sources`, `--disable-slash-commands`, `--no-session-persistence`. **`--bare` es incompatible con el login por suscripción.** `codex` y `copilot` no estaban instalados: sus flags siguen sin verificar.

## Críticos y altos

| Tema | Origen | Resolución en spec v1 |
|---|---|---|
| Shims `.cmd` frágiles, CVE-2024-24576, límite de 8 191 en `.cmd` | R1-H01/H02 | RF-IA1, RF-IA2, Rust ≥ 1.77.2, test de lista blanca |
| `Confirmed` no protege contra la UI (XSS / IPC) | R1-H09, R2-H13 | §6: diálogo nativo desde Rust, token de un solo uso ligado a revisión |
| Lista negra de entorno insuficiente, variables que desvían facturación | R1-H03 | RF-IA3: lista blanca por proveedor y lista de variables prohibidas |
| `claude` sigue cargando hooks, MCP y CLAUDE.md; `--bare` rompe OAuth | R1-H04 | §5.2: flags verificados, `--bare` prohibido, directorio vacío |
| Flags de Codex/Copilot sin verificar; sandbox de Windows distinto | R1-H05, R3-H3/H10 | §5.2, spike M0, experimental si no se verifica |
| Inyección de prompt, secretos en el diff | R1-H06 | §5.1: marcado no fiable, exclusiones, redactor, vista previa |
| Auto envía código a otro proveedor sin consentimiento | R1-H07, R3-H11 | RF-IA9 y §5.1: consentimiento por proveedor, Auto solo por «no instalado» |
| Términos de uso sin analizar | R1-H08, R3-H10 | RF-IA10 y tarea M0 |
| `PathSafety` incompleto (NTFS, ADS, 8.3, symlinks, NFD) | R1-H10 | §4.2 lista cerrada |
| Inyección vía pathspec y refs | R1-H11 | §4.1: literal-pathspecs, `--`, `check-ref-format` |
| Repos hostiles ejecutan código vía git | R1-H12, R2-H17 | §4.1 lecturas endurecidas, §6 repo malicioso |
| Job Object incompleto, `index.lock` huérfano | R1-H13, R2-H01 | §3.1, §9 |
| CRLF / autocrlf sin regla | R1-H16 | §4.1: parches desde bytes de `git diff` |
| Ciclo de vida de sesión al cerrar pestaña | R2-H01 | §3.1 |
| Concurrencia ambigua | R2-H02 | §3.2 |
| Vigilancia de archivos sin diseño | R2-H03, R3-H9 | §8 |
| Sin contrato IPC | R2-H04 | §3.3 |
| Rendimiento sin cifras | R2-H05, R3-H8 | §9 NF-1…NF-8 |
| Estado de pestañas e identidad del repo | R2-H07, R1-H23 | §3.2, §8 |
| Cola y cancelación de IA | R2-H08 | RF-IA6, RF-IA8 |
| Contradicciones ARM64 y D3 | R3-H1, R2-H16 | D3 cerrada; D7 ARM64 mejor esfuerzo |
| Alcance excesivo para una sola 0.1 | R3-H2 | D13: alfas publicables; §1.1 paridad vs novedad |
| «Heredados íntegros» no trazable | R3-H4 | §4: G-01…G-15; tarea de matriz de trazabilidad |
| Fixtures dependen del Swift que se retira | R3-H6 | §11: la fuente de verdad es git |
| Dependencias M4/M6 mal ordenadas | R3-H7 | plan.md: dependencias explícitas |

## Medios y bajos (resumen)
Todos aceptados salvo los siguientes, que se **aplazan o rechazan**:

- Mica/vibrancy (R3-H13): aplazado a v0.2 (D11), propuesta del PM.
- Pruebas con CLI reales como criterio automático (R1-H25, R2-H16): job manual con checklist, no criterio de salida.
- Personalización de atajos (R2-H12): posterior a v0.1; solo registro único.
- Migración de ajustes Swift (R3-H16): solo recientes y favoritos, que viven en un JSON (no en UserDefaults).

## Dudas que quedan abiertas
- ~~macOS 13+ o 15+~~: **resuelto, 15+** (D15).
- Uso en empresa (D14): añade RF-IA10 ampliado, RF-IA11, RF-IA12, firma obligatoria, MSI y red corporativa. Preguntas en spec §13.
- ¿Cuál es el sandbox real de Codex en Windows? Lo dirá el spike.
- ¿Copilot acepta prompt por stdin? Lo dirá el spike.
