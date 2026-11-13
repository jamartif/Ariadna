# Política de firma de código (Code signing policy)

**Ariadna** es un cliente Git de escritorio para macOS y Windows, de código abierto y con licencia [MIT](../LICENSE).

> **Free code signing provided by [SignPath.io](https://signpath.io/), certificate by [SignPath Foundation](https://signpath.org/).**

## Qué se firma

- El instalador NSIS de Windows (`Ariadna_<versión>_x64-setup.exe`) y el ejecutable que contiene.
- Los artefactos se construyen **únicamente** en GitHub Actions a partir de este repositorio, en el workflow [`bundle.yml`](../.github/workflows/bundle.yml), y la firma se solicita para esa ejecución concreta.
- macOS no está cubierto por esta política: el `.dmg` se distribuye con firma ad-hoc hasta que el proyecto disponga de un certificado Developer ID de Apple.

## Equipo y responsabilidades

El proyecto lo mantiene una sola persona, que ejerce los tres roles:

- **Committers y revisores:** [@jamartif](https://github.com/jamartif) — todo cambio entra por *pull request* y lo revisa él antes de fusionar.
- **Aprobadores de firma:** [@jamartif](https://github.com/jamartif) — cada solicitud de firma se aprueba a mano, release por release.

Requisitos que se cumplen: autenticación multifactor obligatoria en GitHub y en SignPath, y solo se firma software construido desde este repositorio.

## Qué garantiza la firma

La firma acredita que el binario procede de una compilación automática de este repositorio en el commit indicado en la release. **No** es una auditoría de seguridad ni una garantía de ausencia de errores.

## Privacidad

**This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.**

En la práctica:

- **Git:** las operaciones de red (fetch, pull, push, clonar, pull requests) las ejecuta el `git` del sistema contra los remotos que el usuario configura. Ariadna no envía nada por su cuenta.
- **IA (opcional y desactivada hasta que el usuario la activa):** al pulsar «Generar con IA» se envía al CLI oficial elegido (Claude, Codex o Copilot) el diff preparado y los últimos asuntos de commit, para proponer un mensaje. Nunca se envía nada sin esa acción. Cada proveedor aplica su propia política; Ariadna no guarda ni registra tokens.
- **Credenciales:** Ariadna no almacena tokens. Los PAT se entregan a `git` en memoria mediante un `GIT_ASKPASS` temporal que se borra al terminar.
- **Telemetría:** no existe. No hay analítica, ni informes de fallos automáticos, ni comprobación de actualizaciones.

## Cómo verificar una descarga

Cada release publica `SHA256SUMS.txt`. En Windows:

```powershell
Get-FileHash .\Ariadna_0.1.0_x64-setup.exe -Algorithm SHA256
```

En macOS:

```bash
shasum -a 256 Ariadna_0.1.0_universal.dmg
```

Compara el resultado con el fichero de la release. A partir de la primera versión firmada, además se puede comprobar la firma Authenticode:

```powershell
Get-AuthenticodeSignature .\Ariadna_0.1.0_x64-setup.exe
```
