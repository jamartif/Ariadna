# Probar Ariadna en Windows sin instalarla en tu equipo

Tu PC tiene **Smart App Control** activo: bloquea cualquier ejecutable sin firma de una CA pública, y no hay forma de saltarlo (`Ejecutar de todas formas` no existe para esto). Estas son las vías que **sí** funcionan hoy, sin tocar la configuración de tu equipo ni renunciar a poder reactivar esa protección algún día.

## Opción 1 — Windows Sandbox (recomendada para probar)

Windows Sandbox arranca un Windows limpio y desechable cada vez, en una ventana. Todo lo que instales ahí desaparece al cerrarla, y tus archivos no se tocan. Tu equipo es **Windows 11 Pro**, así que está disponible.

**Activarla una sola vez** (requiere administrador y reiniciar):

```powershell
# En PowerShell como administrador
Enable-WindowsOptionalFeature -Online -FeatureName "Containers-DisposableClientVM" -All
```

Reinicia. Después, guarda el instalador en una carpeta (por ejemplo `C:\Users\javie\Downloads`) y abre `scripts/ariadna-sandbox.wsb` (doble clic). Se abre un escritorio limpio con esa carpeta compartida en el escritorio (`Desktop`) y la descarga ya iniciada.

Dentro del sandbox, instala con doble clic en `Ariadna_0.1.0_x64-setup.exe`. **Comprobación honesta:** una imagen de Sandbox recién creada no tiene Smart App Control activo, así que el instalador debería ejecutarse; si alguna vez lo bloqueara, es la misma limitación que en tu equipo y habría que esperar a la firma.

Limitaciones del sandbox: sin GPU, sin acceso a tus repositorios reales (solo a la carpeta compartida), y se reinicia de cero cada vez. Sirve para ver la interfaz, probar el instalador y comprobar que arranca.

## Opción 2 — ZIP portable (sin instalador)

Cada release incluye `Ariadna_<versión>_x64-portable.zip` con el ejecutable y sus recursos. Se descomprime en cualquier carpeta y se ejecuta `Ariadna.exe`: no escribe en el registro ni en Program Files.

- En un equipo **sin** Smart App Control funciona directamente.
- En tu equipo, el `.exe` sigue estando sin firmar, así que **también se bloquea**. El ZIP no evita esa política; solo evita el instalador.

## Opción 3 — Otra máquina

Cualquier Windows 10/11 sin Smart App Control sirve. Windows 10 no tiene esta característica, así que es la opción más sencilla si tienes un portátil antiguo a mano.

## Lo que NO funciona (para que no pierdas el tiempo)

- **Desactivar Smart App Control**: Microsoft no permite volver a activarlo sin reinstalar Windows. No merece la pena por probar una alfa.
- **Certificado autofirmado**: Smart App Control no lo acepta, solo firmas de una CA pública de confianza.
- **Quitar la marca de la web** (`Unblock` en las propiedades del archivo): eso afecta a SmartScreen, no a Smart App Control.

## Cuando llegue la firma

Con el certificado de SignPath Foundation, el instalador y el `.exe` pasarán a estar firmados por una CA pública y el bloqueo debería desaparecer. La firma no cambia el programa: solo acredita quién lo publicó y que procede de una compilación automática del repositorio.
