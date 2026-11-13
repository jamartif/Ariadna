# Política de privacidad de Ariadna

**Última actualización:** 13 de noviembre de 2026

**Ariadna no recopila, envía ni almacena datos personales. No hay telemetría, ni analítica, ni informes de fallos automáticos, ni comprobación de actualizaciones.**

> This program will not transfer any information to other networked systems unless specifically requested by the user or the person installing or operating it.

## Qué ocurre en tu equipo

Ariadna es una aplicación de escritorio: todo el trabajo (leer repositorios, calcular diffs, gestionar ramas) ocurre en tu ordenador. La configuración y la lista de repositorios recientes se guardan solo en tu disco, dentro del directorio de datos de la aplicación.

## Cuándo hay tráfico de red

Solo cuando tú lo pides:

- **Operaciones de Git** (fetch, pull, push, clonar, pull requests): las ejecuta el `git` de tu sistema contra los remotos que tú has configurado. El proveedor de ese remoto (GitHub, Azure DevOps, GitLab, Bitbucket…) aplica su propia política.
- **IA opcional:** al pulsar «Generar con IA» se envía al CLI oficial que hayas elegido (Claude, Codex o Copilot) el diff preparado y los últimos asuntos de commit, para proponer un mensaje. Está desactivada hasta que la enciendas y requiere consentimiento. Cada proveedor aplica sus propios términos; Ariadna no guarda ni registra credenciales.

## Credenciales

Ariadna **no almacena tokens ni contraseñas**. Los PAT se entregan a `git` en memoria mediante un `GIT_ASKPASS` temporal que se elimina al terminar, y nunca se escriben en logs ni en la configuración de la aplicación.

## Menores

La aplicación no está dirigida a menores y no recopila datos de ningún usuario.

## Cambios

Cualquier cambio en esta política se publicará en este mismo archivo, con la fecha actualizada.

## Contacto

Abre una incidencia en <https://github.com/jamartif/Ariadna/issues>.
