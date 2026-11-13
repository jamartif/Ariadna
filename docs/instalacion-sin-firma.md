# Instalar Ariadna sin firma (Windows y macOS)

Ariadna 0.1.0 se distribuye **sin firma de código**. En Windows eso significa que SmartScreen
avisará (o que Smart App Control bloqueará) y en macOS que Gatekeeper pedirá confirmación.
Esto explica qué pasa, cómo abrirlo si tu equipo lo permite, **qué cambia cuando haya firma
y qué no prometemos**. No es un manual para saltarse la seguridad de un equipo ajeno: si el
equipo es de tu empresa y su política bloquea apps sin firmar, la decisión es de su
administración, no tuya.

> Mientras la 0.1.0 definitiva no se publique, las alfas publicadas (hoy
> [`v0.1.0-alpha.2`](https://github.com/jamartif/Ariadna/releases/tag/v0.1.0-alpha.2)) usan
> **los mismos nombres de fichero** y este mismo procedimiento.

## 0. De dónde sale el instalador

Se descarga de la página de *releases* del repositorio
([github.com/jamartif/Ariadna/releases](https://github.com/jamartif/Ariadna/releases)), no de
sitios de terceros:

| Sistema | Fichero |
|---|---|
| Windows 10 (22H2)/11, x64 | `Ariadna_0.1.0_x64-setup.exe` (NSIS, por usuario, sin administrador) |
| macOS 15+ | `Ariadna_0.1.0_universal.dmg` (arm64 + x86_64) |

Junto a ellos va **`SHA256SUMS.txt`**. Compruébalo antes de instalar:

```sh
# macOS / Linux
shasum -a 256 -c SHA256SUMS.txt

# Windows (PowerShell)
Get-FileHash .\Ariadna_0.1.0_x64-setup.exe -Algorithm SHA256
```

Si la suma no coincide con `SHA256SUMS.txt`, no lo instales: la descarga está corrupta o no
es la nuestra.

## 1. Windows

### 1.1 SmartScreen («Windows protegió tu PC»)

Es el aviso normal de Microsoft Defender SmartScreen para un binario sin reputación. El
instalador está **firmado por nadie**, así que SmartScreen no puede atribuirlo a un editor
con historial.

1. En el diálogo azul, pulsa **Más información**.
2. Aparece el botón **Ejecutar de todas formas**; pulsa y confirma.

Si el `.exe` se descargó pero Windows lo marcó como «procedente de otro equipo» (a veces
implica también que el instalador no arranca), quita esa marca de la descarga:

```powershell
Unblock-File .\Ariadna_0.1.0_x64-setup.exe
```

Eso solo quita la **zona de internet** del fichero; no cambia ninguna decisión de Smart
App Control.

### 1.2 Smart App Control: aquí no hay «Ejecutar de todas formas»

**Smart App Control (SAC)** es una función de Windows 11 (y de Windows 10 22H2 actualizado
en algunas configuraciones) que evalúa cada binario **antes de ejecutarlo**. Según Microsoft
([FAQ oficial de Smart App Control](https://support.microsoft.com/en-us/windows/security/threat-malware-protection/smart-app-control-frequently-asked-questions)):

> «If the security service is unable to make a confident prediction about the app, then
> Smart App Control checks to see if the app has a valid signature. If the app has a valid
> signature, Smart App Control will let it run. If the app is unsigned, or the signature is
> invalid, Smart App Control will consider it untrusted and block it for your protection.»

Traducido: si Microsoft no tiene una predicción de confianza sobre el binario, mira la firma;
si **no hay firma válida**, lo bloquea. No hay botón de excepción por app. En equipos de
desarrollo esto se ve como un error del sistema al ejecutar un `build-script` recién
compilado:

```
Una directiva de Control de aplicaciones bloqueó este archivo. (os error 4551)
```

**Cómo saber si está activo:** *Seguridad de Windows* → *Control de aplicaciones y
navegador* → *Configuración de Smart App Control*. Los estados son *Activado*, *Evaluación*
(aprende unos días y luego se decide) y *Desactivado*.

**Opciones reales, en orden de preferencia:**

1. **Esperar/instalar la versión firmada.** Es la solución de verdad (ver §3).
2. **Desactivarlo** en *Seguridad de Windows* → *Control de aplicaciones y navegador* →
   *Smart App Control*. Microsoft documenta que se puede desactivar ahí; según la misma FAQ,
   volver a activarlo exigía restablecer Windows, aunque actualizaciones recientes permiten
   reactivarlo desde la app de Seguridad. Es una decisión de **seguridad del equipo**: en un
   portátil de empresa, consúltalo con TI antes.
3. **Compilar desde el código** (ver el README, sección *Desarrollo*). En Windows, compilar
   desde WSL evita el bloqueo de los *build scripts*, porque el binario lo produce Linux.

Lo que **no** funciona: renombrar el `.exe`, ejecutarlo «como administrador», desactivar
Defender momentáneamente o pasar el fichero por un antivirus. SAC no mira el nombre ni el
permiso del proceso, mira la firma.

### 1.3 Empresas: AppLocker, WDAC e Intune

En equipos gestionados, además de SAC puede haber **AppLocker**, **WDAC** (política de
control de aplicaciones del sistema) o una política de *Intune*. Esas bloquean por lista de
editores o hashes y **no se desactivan desde el equipo**. Si Ariadna no arranca y SAC está
desactivado, pregunta a TI qué política aplica y si puede autorizar el binario o el editor
firmado.

## 2. macOS

### 2.1 Gatekeeper y la cuarentena

Al abrir el `.dmg` descargado, macOS marca la app con el atributo `com.apple.quarantine` y
Gatekeeper comprueba firma y notarización. Sin firma de *Developer ID* ni notarización, el
sistema dirá algo como «Ariadna no se puede abrir porque no se puede verificar el
desarrollador» (o «está dañada»).

**macOS 15 (Sequoia) cambió el atajo:** el clásico *clic derecho → Abrir* **ya no** salta
Gatekeeper. El camino es:

1. Intenta abrir la app una vez (para que macOS registre el intento).
2. *Ajustes del Sistema* → **Privacidad y seguridad**.
3. Abajo, en *Seguridad*, aparece «Se ha bloqueado el uso de Ariadna…»; pulsa
   **Abrir igualmente** y confirma con la contraseña o Touch ID.

Alternativa por terminal, equivalente a autorizar esa copia concreta:

```sh
xattr -dr com.apple.quarantine /Applications/Ariadna.app
```

Abre el `.dmg` y arrastra `Ariadna.app` a `/Applications`; luego `-dr` borra la cuarentena de
todo el bundle (la `-r` es recursiva).

Para comprobar qué piensa Gatekeeper de la app:

```sh
spctl -a -vvv -t execute /Applications/Ariadna.app
```

Verás `rejected` con el motivo («source=no usable signature» o similar); es lo esperado sin
firma.

### 2.2 Macs gestionados

Un Mac con MDM puede impedir *Abrir igualmente* (el botón no aparece o la política bloquea
apps no notarizadas). En ese caso la única vía es que la empresa autorice la app o que
instale la versión firmada y notarizada. No hay atajo que lo evite, y conviene no intentarlo.

## 3. Qué cambia cuando haya firma

La decisión de producto está en
[`specs/rust/spec.md` §10, D17](../specs/rust/spec.md): las alfas salen sin firmar; para la
0.1.0 se solicita **SignPath Foundation** (gratis para proyectos de código abierto; el repo
es público y tiene licencia MIT).

| Plataforma | Firma prevista | Efecto |
|---|---|---|
| **Windows** | Firma **Authenticode** del `.exe` (instalador y binarios) con certificado de una CA de confianza, vía SignPath Foundation | SmartScreen deja de mostrar el aviso rojo en cuanto el editor tiene reputación; Smart App Control **acepta** el binario por tener firma válida. La reputación de SmartScreen se gana con el tiempo y descargas: la primera versión firmada puede seguir avisando unos días |
| **macOS** | Firma **Developer ID** + **notarización** de Apple | Gatekeeper abre la app sin pasos extra (`spctl` acepta). Requiere **Apple Developer Program** (de pago, ~99 USD/año) y credenciales en el workflow. Hoy la app lleva **firma ad-hoc**, que no basta para Gatekeeper |

Mientras no haya firma: el `.dmg` que se publica lleva firma ad-hoc (no notarizada) y el
instalador de Windows no lleva ninguna.

## 4. Qué NO prometemos

- **No** prometemos que Smart App Control, AppLocker, WDAC, un EDR o un MDM dejen ejecutar
  Ariadna. Una firma reduce los bloqueos por reputación, pero una **política corporativa
  puede prohibir explícitamente** apps no autorizadas; eso no lo resuelve la firma.
- **No** hay versiones para macOS 14 o anterior, ni para Windows ARM64 con garantía (se
  compila, sin soporte), ni instalador MSI, ni despliegue gestionado (fuera de v0.1).
- **No** hay actualizador automático: cada versión se descarga a mano.
- **No** damos soporte a bins «portables» o reconstruidos por terceros; el SHA-256 publicado
  es la referencia.
- **No** garantizamos que la firma esté disponible: si SignPath Foundation no concede el
  proyecto, se publicará sin firmar con esta misma guía (D17), y lo diremos en las notas de
  la versión.
- Una firma dice **quién** publicó el binario, no que sea inofensivo. El código es auditable
  en el repositorio y el binario se construye en CI público; no hay más promesa que esa.

## 5. Si algo falla

- **Windows, error 4551** al ejecutar: Smart App Control (§1.2).
- **Windows, el instalador arranca pero dice que WebView2 falta**: el instalador lo descarga;
  si el equipo no tiene salida a internet, instala primero WebView2 Evergreen desde
  Microsoft.
- **macOS, «dañada»**: casi siempre es cuarentena (§2.1), no corrupción real. Comprueba el
  SHA-256 antes de culpar al fichero.
- **`git` no encontrado** al abrir la app: instala git 2.39 o posterior y reinicia Ariadna;
  Ariadna valida la versión al arrancar y avisa con un error accionable.
