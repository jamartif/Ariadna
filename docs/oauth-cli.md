# Iniciar sesión con GitHub/GitLab al clonar (OAuth device code)

Ariadna puede clonar con tres métodos: un **token de acceso personal (PAT)**, la **sesión que ya
tengas iniciada** en GitHub CLI (`gh`) o GitLab CLI (`glab`), y el **flujo OAuth de código de
dispositivo** (RFC 8628). Esta guía es solo para el tercero, que llega **desactivado** a propósito.

## Por qué hay que configurarlo tú

El flujo device code necesita un `client_id` de una aplicación OAuth registrada en el proveedor.
Ariadna **no puede** registrar aplicaciones en tu nombre ni incluir un `client_id` propio:

- Un `client_id` identifica a la aplicación ante GitHub/GitLab y su registro es un acto del dueño
  de la cuenta (acepta condiciones y, en GitHub, puede requerir verificación de la organización).
- Un `client_id` **no es un secreto**, pero es configuración tuya.

Sin `client_id` configurado, el diálogo de clonado **oculta** la opción OAuth y muestra solo PAT y,
si están instalados, los CLI. Si lo prefieres, usa `gh auth login` / `glab auth login` y elige
«Sesión de … CLI»: no hay que registrar nada.

## 1. Registrar la aplicación en GitHub

1. GitHub › **Settings** › **Developer settings** › **OAuth Apps** › **New OAuth App**.
2. **Application name**: `Ariadna` (lo que quieras).
3. **Homepage URL**: `https://github.com/jamartif/Ariadna` (o la tuya).
4. **Authorization callback URL**: el flujo device code **no** usa redirección, pero el campo es
   obligatorio; pon la misma homepage (`https://github.com/jamartif/Ariadna`).
5. Crea la app y, en su página, marca **Enable Device Flow**. Sin esta casilla GitHub responde
   `device_flow_disabled` y Ariadna te lo dirá.
6. Copia el **Client ID**. El **Client secret** no se usa: no lo pegues en ningún sitio.

Permisos que pedirá Ariadna: `repo` (token clásico). Un token con más permisos de los necesarios
no aporta nada al clonado; con `repo` se puede clonar, hacer `fetch`/`push` y abrir PR en repos
privados.

## 2. Registrar la aplicación en GitLab

1. GitLab › **Preferences** (tu avatar) › **Applications**.
2. **Name**: `Ariadna`.
3. **Redirect URI**: `https://gitlab.com/` (no se usa en device flow, pero es obligatorio).
4. **Confidential**: desmarcado (cliente público; no hay `client_secret`).
5. Marca los scopes **`read_repository`** y **`write_repository`** (son los que pide Ariadna).
6. Guarda y copia el **Application ID** (es el `client_id`).

El flujo device code de GitLab está disponible en gitlab.com. En una instancia autoalojada
depende de su versión y configuración; Ariadna hoy solo lo ofrece para `gitlab.com`.

## 3. Configurarlo en Ariadna

**Ajustes › Cuentas** → sección «Inicio de sesión con OAuth»:

- **client_id de GitHub**: el `Iv1.…` de la app del paso 1.
- **client_id de GitLab**: el Application ID del paso 2.

Se guarda en `settings.json` (`oauthGitHubClientId` / `oauthGitLabClientId`, sin secretos). Si se
deja vacío, la opción OAuth desaparece del diálogo de clonado.

Alternativa sin tocar la interfaz: edita `settings.json` en la carpeta de datos de la app y
reinicia.

## 4. Qué hace Ariadna exactamente

1. `POST https://github.com/login/device/code` (o `https://gitlab.com/oauth/authorize_device`)
   con el `client_id` y el scope. Vuelve `device_code`, `user_code`, `verification_uri`,
   `expires_in` e `interval`.
2. La interfaz muestra el `user_code` y abre `verification_uri` en el navegador del sistema
   (`https://github.com/login/device` o `https://gitlab.com/-/device`).
3. Ariadna sondea `https://github.com/login/oauth/access_token` (o `https://gitlab.com/oauth/token`)
   cada `interval` segundos: `authorization_pending` sigue esperando, `slow_down` suma 5 s,
   `expired_token` pide empezar de nuevo, `access_denied` lo dice y un `access_token` termina.
4. El `access_token` **vive solo en memoria de Rust**, ligado a un identificador opaco de flujo
   (`flowId`) que es lo único que conoce la interfaz. Se borra (`zeroize`) al usarlo en el
   `git clone` o al cerrar el diálogo; nunca se escribe en disco, ni en `settings.json`, ni en un
   registro, ni viaja en la URL (se entrega a git por `GIT_ASKPASS` temporal).

Límites que conviene saber:

- El token se pide para una operación: **no hay refresco ni caducidad gestionada**. Cuando expire,
  vuelve a iniciar sesión.
- Solo `github.com` y `gitlab.com` (los endpoints son fijos). GitHub Enterprise y GitLab
  autoalojado no están soportados todavía.
- El sondeo lo hace la interfaz, así que si cierras el diálogo el flujo se cancela (Rust olvida el
  `device_code` y el token pendiente).

## 5. Si algo falla

| Mensaje | Qué hacer |
|---|---|
| «La aplicación OAuth no admite el flujo de código de dispositivo» | Marca **Enable Device Flow** en la app de GitHub (`device_flow_disabled`) o revisa los scopes en GitLab |
| «Falta el client_id de la aplicación OAuth» | Pega el `client_id` en Ajustes › Cuentas |
| «El código caducó» | Vuelve a pulsar «Iniciar sesión en el navegador» (`expires_in` se agotó) |
| «Has denegado el acceso» | Autoriza la app en la página del navegador |
| «La autorización OAuth ha caducado» al clonar | El token ya se usó o pasaron más de 10 minutos: repite el inicio de sesión |

## 6. Alternativa sin OAuth

- **GitHub CLI**: `gh auth login` y en el diálogo elige «Sesión de GitHub CLI».
- **GitLab CLI**: `glab auth login` y elige «Sesión de GitLab CLI».
- **PAT**: token de un solo uso; no se guarda. Los permisos mínimos están en la propia ayuda del
  diálogo y en Ajustes › Cuentas.
