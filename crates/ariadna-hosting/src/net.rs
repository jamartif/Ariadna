//! Proxy y certificados del sistema para el cliente HTTP (M7, «mejor esfuerzo»).
//!
//! §4.4 exige TLS siempre verificado y, cuando la máquina vive detrás de un proxy
//! corporativo (o inspecciona TLS con una CA propia), Ariadna tiene que seguir
//! funcionando sin que el usuario configure nada a mano. Este módulo construye el
//! cliente `reqwest` **respetando lo que ya está en el entorno**, con las mismas
//! variables que usan `git`, `curl` y los CLI de IA:
//!
//! * **Proxy**: [`VAR_HTTPS_PROXY`], [`VAR_HTTP_PROXY`] y `ALL_PROXY` (en mayúsculas
//!   o minúsculas). `https://` usa el proxy HTTPS; `http://` solo existe en las
//!   pruebas contra *loopback*. [`VAR_NO_PROXY`] excluye hosts por sufijo, con
//!   `*` como comodín de «todo directo».
//! * **Certificados extra**: [`VAR_SSL_CERT_FILE`] (un PEM con uno o varios
//!   certificados) y `SSL_CERT_DIR` (directorio con PEM sueltos). Se **añaden** a
//!   las raíces del sistema, no las sustituyen: en Windows y macOS las raíces
//!   salen del almacén del sistema vía `rustls-platform-verifier` (que incluye las
//!   CA instaladas por la empresa), así que aquí no se vuelve a leer ese almacén.
//!
//! Lo que **no** hace, y conviene decirlo: no desactiva la verificación de TLS,
//! no acepta certificados inválidos «porque hay proxy», no inventa un proxy si
//! las variables apuntan a un sitio inalcanzable y no toca configuración de git
//! ni de los CLI. Un proxy mal escrito no se adivina: [`build_http_client`]
//! devuelve un error que nombra la variable.
//!
//! Hoy `http.rs` sigue construyendo su propio cliente con la política de
//! redirecciones y los tiempos de §4.4; [`build_http_client`] es la pieza que ese
//! constructor puede adoptar (proxy + CA en un solo sitio) sin cambiar su
//! comportamiento de red. Las pruebas de este módulo usan un proxy local que
//! recibe `CONNECT` y un certificado de prueba firmado al vuelo: no hay red real.

use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

use reqwest::{Certificate, Client, ClientBuilder, NoProxy, Proxy};
use url::Url;

use crate::error::{HostingError, HostingResult};

static TLS_PROVIDER: Once = Once::new();

fn install_crypto_provider() {
    // El crate compila `reqwest` con `rustls-no-provider`: hay que elegir el
    // proveedor criptográfico antes de construir un cliente. `ring` es el que
    // usa `http.rs` (sin NASM en Windows). Si otro componente ya instaló el suyo,
    // se respeta.
    TLS_PROVIDER.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Variable de proxy para peticiones `https://`.
pub const VAR_HTTPS_PROXY: &str = "HTTPS_PROXY";
/// Variable de proxy para peticiones `http://` (solo *loopback* en pruebas).
pub const VAR_HTTP_PROXY: &str = "HTTP_PROXY";
/// Proxy para cualquier esquema; se usa solo si el específico no está definido.
pub const VAR_ALL_PROXY: &str = "ALL_PROXY";
/// Lista de hosts que van directos, sin proxy.
pub const VAR_NO_PROXY: &str = "NO_PROXY";
/// Fichero PEM con certificados de confianza adicionales.
pub const VAR_SSL_CERT_FILE: &str = "SSL_CERT_FILE";
/// Directorio con ficheros PEM de confianza adicionales.
pub const VAR_SSL_CERT_DIR: &str = "SSL_CERT_DIR";

/// Lo que se lee del entorno (o se impone en las pruebas). Todo opcional.
///
/// Separar esto de `std::env` permite probar el enrutado y los certificados sin
/// carreras entre tests que comparten proceso.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetSettings {
    pub https_proxy: Option<String>,
    pub http_proxy: Option<String>,
    pub all_proxy: Option<String>,
    pub no_proxy: Option<String>,
    pub ssl_cert_file: Option<PathBuf>,
    pub ssl_cert_dir: Option<PathBuf>,
}

impl NetSettings {
    /// Lee las variables del proceso. Las minúsculas cuentan cuando la mayúscula
    /// no está definida (`git` y `curl` aceptan ambas).
    pub fn from_env() -> Self {
        Self {
            https_proxy: env_var(VAR_HTTPS_PROXY),
            http_proxy: env_var(VAR_HTTP_PROXY),
            all_proxy: env_var(VAR_ALL_PROXY),
            no_proxy: env_var(VAR_NO_PROXY),
            ssl_cert_file: env_var(VAR_SSL_CERT_FILE).map(PathBuf::from),
            ssl_cert_dir: env_var(VAR_SSL_CERT_DIR).map(PathBuf::from),
        }
    }

    /// URL del proxy que corresponde a un esquema, con la precedencia habitual:
    /// el específico del esquema manda y `ALL_PROXY` es el respaldo.
    #[cfg(test)]
    fn proxy_for(&self, scheme: &str) -> Option<&str> {
        let specific = match scheme {
            "https" => self.https_proxy.as_deref(),
            "http" => self.http_proxy.as_deref(),
            _ => None,
        };
        specific.or(self.all_proxy.as_deref())
    }

    fn any_proxy(&self) -> bool {
        self.https_proxy.is_some() || self.http_proxy.is_some() || self.all_proxy.is_some()
    }
}

fn env_var(name: &str) -> Option<String> {
    let upper = std::env::var(name).ok();
    let lower = std::env::var(name.to_ascii_lowercase()).ok();
    upper
        .or(lower)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

/// Reglas de proxy ya validadas, para el cierre de `Proxy::custom`.
#[derive(Clone)]
struct Routing {
    https: Option<Url>,
    http: Option<Url>,
    all: Option<Url>,
}

impl Routing {
    /// Valida cada URL configurada. Un valor sin esquema (`proxy:3128`, lo que
    /// ponen muchas empresas) se interpreta como `http://`. El mensaje de error
    /// nombra la variable y **no** repite el valor: puede llevar credenciales.
    fn new(settings: &NetSettings) -> HostingResult<Option<Self>> {
        if !settings.any_proxy() {
            return Ok(None);
        }
        Ok(Some(Self {
            https: parse_proxy(settings.https_proxy.as_deref(), VAR_HTTPS_PROXY)?,
            http: parse_proxy(settings.http_proxy.as_deref(), VAR_HTTP_PROXY)?,
            all: parse_proxy(settings.all_proxy.as_deref(), VAR_ALL_PROXY)?,
        }))
    }

    fn for_scheme(&self, scheme: &str) -> Option<Url> {
        let specific = match scheme {
            "https" => self.https.clone(),
            "http" => self.http.clone(),
            _ => None,
        };
        specific.or_else(|| self.all.clone())
    }
}

fn parse_proxy(value: Option<&str>, var: &str) -> HostingResult<Option<Url>> {
    let Some(raw) = value else {
        return Ok(None);
    };
    let candidate = if raw.contains("://") {
        raw.to_owned()
    } else {
        format!("http://{raw}")
    };
    Url::parse(&candidate).map(Some).map_err(|_| {
        HostingError::invalid(format!(
            "{var} no es una dirección de proxy válida (se espera http://host:puerto)."
        ))
    })
}

/// Certificados adicionales de confianza leídos de `SSL_CERT_FILE` / `SSL_CERT_DIR`.
///
/// * Un fichero declarado en `SSL_CERT_FILE` que no se puede leer o no contiene
///   ningún PEM **es un error**: el usuario cree que su CA está cargada y no lo
///   estaría. Mejor decirlo que fallar luego con «certificado no válido».
/// * `SSL_CERT_DIR` es de mejor esfuerzo (como hace OpenSSL): se leen los ficheros
///   que se dejen leer y se ignoran los que no son PEM; un directorio ausente no
///   es un error.
fn extra_certificates(settings: &NetSettings) -> HostingResult<Vec<Certificate>> {
    let mut certs = Vec::new();
    if let Some(path) = &settings.ssl_cert_file {
        let bytes = std::fs::read(path).map_err(|_| {
            HostingError::invalid(format!(
                "{VAR_SSL_CERT_FILE} apunta a un fichero que no se puede leer ({path:?})."
            ))
        })?;
        let bundle = Certificate::from_pem_bundle(&bytes).map_err(|_| {
            HostingError::invalid(format!(
                "{VAR_SSL_CERT_FILE} no contiene ningún certificado PEM válido ({path:?})."
            ))
        })?;
        if bundle.is_empty() {
            return Err(HostingError::invalid(format!(
                "{VAR_SSL_CERT_FILE} no contiene ningún certificado PEM válido ({path:?})."
            )));
        }
        certs.extend(bundle);
    }
    if let Some(dir) = &settings.ssl_cert_dir {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                let Ok(bytes) = std::fs::read(&path) else {
                    continue;
                };
                if let Ok(mut bundle) = Certificate::from_pem_bundle(&bytes) {
                    certs.append(&mut bundle);
                }
            }
        }
    }
    Ok(certs)
}

/// Cliente HTTP de producción: proxy y CA del entorno, TLS siempre verificado.
///
/// Los tiempos, redirecciones y reintentos siguen siendo cosa del llamador (§4.4).
pub fn build_http_client() -> HostingResult<Client> {
    apply_system_networking(default_builder())?
        .build()
        .map_err(|_| HostingError::network("No se pudo preparar la conexión segura."))
}

/// Añade proxy y certificados del sistema a un constructor ya configurado.
///
/// Es la pieza que `http.rs` puede adoptar sin cambiar su política de redirecciones,
/// tiempos o reintentos: solo llamar `apply_system_networking(builder)?` antes de
/// `build()`. No añade cabeceras, no cambia la verificación de TLS y no sustituye
/// las raíces del sistema: **las amplía**.
pub fn apply_system_networking(builder: ClientBuilder) -> HostingResult<ClientBuilder> {
    install_crypto_provider();
    configure(builder, &NetSettings::from_env())
}

/// Igual que [`build_http_client`] pero con ajustes explícitos (solo pruebas).
#[cfg(test)]
pub fn build_with(settings: &NetSettings) -> HostingResult<Client> {
    install_crypto_provider();
    configure(default_builder(), settings)?
        .build()
        .map_err(|_| HostingError::network("No se pudo preparar la conexión segura."))
}

fn default_builder() -> ClientBuilder {
    Client::builder()
        .user_agent(concat!("Ariadna/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(10))
}

fn configure(mut builder: ClientBuilder, settings: &NetSettings) -> HostingResult<ClientBuilder> {
    if settings
        .no_proxy
        .as_deref()
        .is_some_and(|list| list.trim() == "*")
    {
        // `NO_PROXY=*` significa «todo directo»: no se añade ninguna regla y se
        // desactiva la detección del sistema, para que nada se cuele.
        builder = builder.no_proxy();
    } else if let Some(routing) = Routing::new(settings)? {
        let proxy = Proxy::custom(move |url: &Url| routing.for_scheme(url.scheme()));
        let proxy = match settings.no_proxy.as_deref().map(str::trim) {
            Some(list) if !list.is_empty() => proxy.no_proxy(NoProxy::from_string(list)),
            _ => proxy,
        };
        builder = builder.proxy(proxy);
    }

    for cert in extra_certificates(settings)? {
        builder = builder.add_root_certificate(cert);
    }

    Ok(builder)
}

#[cfg(test)]
mod tests {
    // Las pruebas de este módulo usan `unwrap`/`expect` a propósito: si el fixture
    // falla, se quiere ver el pánico con su línea. El código de producción no.
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use std::sync::Arc;

    use rustls::pki_types::{CertificateDer, PrivatePkcs8KeyDer};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    use super::*;

    fn settings_with_proxy(proxy: String) -> NetSettings {
        NetSettings {
            https_proxy: Some(proxy),
            ..NetSettings::default()
        }
    }

    #[test]
    fn enruta_por_esquema_con_all_proxy_de_respaldo() {
        let s = NetSettings {
            https_proxy: Some("http://proxy-https:8443".into()),
            http_proxy: None,
            all_proxy: Some("http://proxy-all:3128".into()),
            ..NetSettings::default()
        };
        let routing = Routing::new(&s).unwrap().unwrap();
        assert_eq!(
            routing
                .for_scheme("https")
                .map(|u| u.to_string())
                .as_deref(),
            Some("http://proxy-https:8443/")
        );
        assert_eq!(
            routing.for_scheme("http").map(|u| u.to_string()).as_deref(),
            Some("http://proxy-all:3128/")
        );
        // Sin proxy propio para el esquema ni ALL_PROXY, la petición va directa.
        let only_https = NetSettings {
            https_proxy: Some("proxy.example:3128".into()),
            ..NetSettings::default()
        };
        assert_eq!(only_https.proxy_for("https"), Some("proxy.example:3128"));
        let routing = Routing::new(&only_https).unwrap().unwrap();
        assert!(routing.for_scheme("https").is_some());
        assert!(routing.for_scheme("http").is_none());
    }

    #[test]
    fn un_proxy_sin_esquema_se_interpreta_como_http() {
        let r = Routing::new(&settings_with_proxy("10.0.0.5:3128".into()))
            .unwrap()
            .unwrap();
        assert_eq!(
            r.for_scheme("https").map(|u| u.to_string()).as_deref(),
            Some("http://10.0.0.5:3128/")
        );
    }

    #[test]
    fn un_proxy_mal_escrito_es_un_error_que_no_repite_el_valor() {
        let err = build_with(&settings_with_proxy("http://usuario:secreto@".into())).unwrap_err();
        let text = format!("{err:?}");
        assert!(
            text.contains(VAR_HTTPS_PROXY),
            "el mensaje nombra la variable: {text}"
        );
        assert!(!text.contains("secreto"), "el valor no se repite: {text}");
        // Sin proxy configurado no se añade ninguna regla.
        assert!(Routing::new(&NetSettings::default()).unwrap().is_none());
    }

    #[tokio::test]
    async fn el_proxy_local_recibe_connect_con_el_host_del_proveedor() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let seen = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buf = Vec::new();
            loop {
                let mut chunk = [0_u8; 256];
                let n = socket.read(&mut chunk).await.unwrap_or(0);
                buf.extend_from_slice(&chunk[..n]);
                if n == 0 || buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            // El proxy rechaza el túnel: la petición no puede llegar a destino.
            let _ = socket
                .write_all(b"HTTP/1.1 502 Bad Gateway\r\ncontent-length: 0\r\n\r\n")
                .await;
            String::from_utf8_lossy(&buf).to_string()
        });

        let client = build_with(&settings_with_proxy(format!("http://{addr}"))).unwrap();
        let failed = client.get("https://api.github.com/rate_limit").send().await;
        // Lo importante es que la petición pasó por el proxy local y no salió a la red.
        assert!(
            failed.is_err(),
            "el túnel rechazado no puede dar una respuesta correcta"
        );
        let first_line = seen.await.unwrap();
        assert!(
            first_line.starts_with("CONNECT api.github.com:443 HTTP/1.1"),
            "se esperaba CONNECT, llegó: {first_line:?}"
        );
    }

    #[tokio::test]
    async fn no_proxy_salta_el_proxy_para_el_host_excluido() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let touched = tokio::spawn(async move {
            // Si algo llega aquí, NO_PROXY no se respetó. Se agota el tiempo.
            matches!(
                tokio::time::timeout(Duration::from_secs(2), listener.accept()).await,
                Ok(Ok(_))
            )
        });

        let mut settings = settings_with_proxy(format!("http://{addr}"));
        settings.no_proxy = Some("*".into());
        let client = build_with(&settings).unwrap();
        // `*` deja la petición directa: no hay proxy en el camino.
        let _ = client
            .get("https://no-existe.invalid/")
            .send()
            .await
            .map(|_| ());
        assert!(
            !touched.await.unwrap(),
            "el proxy no debe recibir nada con NO_PROXY=*"
        );
    }

    #[tokio::test]
    async fn ssl_cert_file_anade_la_ca_de_prueba_y_el_tls_verifica() {
        // --- certificado de prueba: una CA y una hoja firmada por ella (claves en memoria)
        let ca_key = rcgen::KeyPair::generate().unwrap();
        let mut ca = rcgen::CertificateParams::new(Vec::<String>::new()).unwrap();
        ca.is_ca = rcgen::IsCa::Ca(rcgen::BasicConstraints::Unconstrained);
        ca.distinguished_name
            .push(rcgen::DnType::CommonName, "Ariadna CA de prueba");
        let ca_cert = ca.self_signed(&ca_key).unwrap();

        let server_key = rcgen::KeyPair::generate().unwrap();
        let mut server = rcgen::CertificateParams::new(vec!["127.0.0.1".to_owned()]).unwrap();
        server
            .distinguished_name
            .push(rcgen::DnType::CommonName, "ariadna localhost");
        server
            .extended_key_usages
            .push(rcgen::ExtendedKeyUsagePurpose::ServerAuth);
        let server_cert = server
            .signed_by(&server_key, &rcgen::Issuer::from_params(&ca, &ca_key))
            .unwrap();

        let _ = rustls::crypto::ring::default_provider().install_default();
        let server_config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(server_cert.der().to_vec())],
                PrivatePkcs8KeyDer::from(server_key.serialize_der()).into(),
            )
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(server_config));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = tokio::spawn(async move {
            // Dos conexiones: la primera (sin la CA cargada) se rechaza en el
            // handshake; la segunda, con `SSL_CERT_FILE`, tiene que completarse.
            for _ in 0..2 {
                let (socket, _) = listener.accept().await.unwrap();
                let Ok(mut tls) = acceptor.accept(socket).await else {
                    continue;
                };
                let mut buf = Vec::new();
                let mut chunk = [0_u8; 512];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    let n = tls.read(&mut chunk).await.unwrap_or(0);
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                if tls
                    .write_all(b"HTTP/1.1 204 No Content\r\ncontent-length: 0\r\n\r\n")
                    .await
                    .is_err()
                {
                    continue;
                }
                let _ = tls.shutdown().await;
            }
        });

        let dir = tempfile::tempdir().unwrap();
        let ca_path = dir.path().join("ariadna-ca-de-prueba.pem");
        std::fs::write(&ca_path, ca_cert.pem()).unwrap();

        // Sin SSL_CERT_FILE el handshake tiene que fallar: la CA no es del sistema.
        let plain = build_with(&NetSettings::default()).unwrap();
        let rejected = plain.get(format!("https://127.0.0.1:{port}/")).send().await;
        assert!(
            rejected.is_err(),
            "una CA desconocida no puede validar el TLS"
        );

        // Con SSL_CERT_FILE apuntando al PEM de prueba, el cliente confía en ella.
        let settings = NetSettings {
            ssl_cert_file: Some(ca_path),
            ..NetSettings::default()
        };
        let trusting = build_with(&settings).unwrap();
        let reply = trusting
            .get(format!("https://127.0.0.1:{port}/"))
            .send()
            .await
            .unwrap();
        assert_eq!(reply.status(), 204);
        server.await.unwrap();
    }

    #[test]
    fn ssl_cert_file_inexistente_o_vacio_da_error_claro() {
        let missing = NetSettings {
            ssl_cert_file: Some(PathBuf::from("no/existe/ca.pem")),
            ..NetSettings::default()
        };
        let err = build_with(&missing).unwrap_err();
        assert!(format!("{err:?}").contains(VAR_SSL_CERT_FILE));

        let dir = tempfile::tempdir().unwrap();
        let empty = dir.path().join("basura.pem");
        std::fs::write(&empty, b"no soy un certificado").unwrap();
        let err = build_with(&NetSettings {
            ssl_cert_file: Some(empty),
            ..NetSettings::default()
        })
        .unwrap_err();
        assert!(format!("{err:?}").contains(VAR_SSL_CERT_FILE));
    }

    #[test]
    fn un_cliente_sin_configuracion_se_construye() {
        // Sin proxy ni CA el cliente es válido y no se abre ninguna conexión.
        assert!(build_with(&NetSettings::default()).is_ok());
    }
}
