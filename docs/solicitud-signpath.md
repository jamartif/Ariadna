# Solicitud a SignPath Foundation — texto listo para enviar

Rellena el formulario de <https://signpath.org/> (botón **Apply**) con estos datos. Está redactado para copiar y pegar.

## Datos del proyecto

| Campo | Valor |
|---|---|
| Nombre del proyecto | Ariadna |
| Repositorio | https://github.com/jamartif/Ariadna |
| Licencia | MIT (OSI) |
| Lenguaje / stack | Rust + Tauri 2, interfaz Svelte 5 |
| Plataformas | Windows 10/11 (x64) y macOS 15+ |
| Versión actual | `v0.1.0-alpha.3` (pre-release) |
| Releases | https://github.com/jamartif/Ariadna/releases |
| Política de firma | `docs/firma-de-codigo.md` (publicada en el README) |
| Contacto | el correo de la cuenta de GitHub |

## Descripción (para el campo de descripción)

> Ariadna es un cliente Git de escritorio, en español, para macOS y Windows. Usa el `git` del sistema (hooks, credenciales y configuración reales) y añade una interfaz para preparar cambios por hunk y por línea, historial, ramas, remotos, resolución de conflictos a tres paneles, stash, clonado con PAT y pull requests de GitHub y Azure DevOps. Opcionalmente propone mensajes de commit con los CLI oficiales de Claude, Codex o Copilot, siempre con consentimiento y sin guardar credenciales. Es software libre con licencia MIT, sin código propietario ni telemetría.

## Por qué se solicita

> Los instaladores de Windows se construyen en GitHub Actions y se publican sin firmar. Windows 11 con *Smart App Control* activado bloquea cualquier ejecutable sin firma de una CA pública (eventos CodeIntegrity 3077/3033), de modo que los usuarios no pueden instalar la aplicación. La firma no cambia el binario: solo acredita que procede de una compilación automática de este repositorio.

## Declaración de cumplimiento

Marca lo que corresponda (todo es cierto en este proyecto):

- [x] Sin malware ni programas potencialmente no deseados.
- [x] Licencia aprobada por OSI (MIT), sin doble licencia comercial.
- [x] Sin componentes propietarios; solo bibliotecas de terceros con licencia libre.
- [x] Proyecto mantenido y ya publicado (releases con binarios).
- [x] El equipo de firma es el mismo que desarrolla y posee el repositorio.
- [x] Solo se firma software compilado desde este repositorio.
- [x] MFA activada en GitHub y en SignPath.
- [x] Política de firma publicada en la página del proyecto.
- [x] Política de privacidad incluida (sin telemetría; solo red cuando el usuario lo pide).
- [x] Cada release se aprueba a mano antes de firmar.

## Pasos que hay que hacer antes de enviar

1. **Activar MFA** en GitHub (Ajustes → Password and authentication). Es obligatorio para el equipo.
2. Comprobar que el README enlaza `docs/firma-de-codigo.md` (ya está hecho en la rama `rust-migration`; se publicará en `main` al fusionar la PR #13).
3. Enviar el formulario y esperar respuesta.

## Cuando lo aprueben (lo hago yo)

1. Crear en SignPath la organización y el proyecto, y un *signing policy* para el instalador.
2. Generar un token de API y guardarlo como *secret* `SIGNPATH_API_TOKEN` en GitHub (nunca en el código).
3. Añadir al workflow `bundle.yml` el envío del artefacto y la descarga del firmado, con aprobación manual por release.
4. Comprobar la firma con `Get-AuthenticodeSignature` y publicar la release con el instalador firmado.
