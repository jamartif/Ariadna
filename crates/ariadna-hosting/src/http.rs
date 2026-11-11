//! Cliente HTTP común: `reqwest` + `rustls`, con los límites de seguridad de red de §4.4.
//!
//! * TLS siempre verificado (raíces del sistema); `http://` solo en pruebas contra *loopback*
//!   (`HttpConfig::allow_insecure_loopback`).
//! * Solo se contacta el origen (esquema + host + puerto) del proveedor; las redirecciones solo
//!   se siguen dentro de ese origen y los enlaces `next` de paginación se validan igual.
//! * Tiempo máximo por petición (30 s por defecto) y **tope de tamaño** de respuesta.
//! * Reintentos acotados: 429 con `Retry-After`, y 502/503/504 y errores de red solo en
//!   lecturas (`GET`). Nunca se reintenta una escritura que pudo llegar al servidor.
//! * No hay registro (`log`/`tracing`) en este crate: ninguna cabecera sale a ningún sitio.

use std::sync::Once;
use std::time::Duration;

use reqwest::header::{HeaderMap, ACCEPT, CONTENT_TYPE, RETRY_AFTER};
use reqwest::{Method, StatusCode};
use url::Url;

use crate::auth::{AuthMethod, Scrubber};
use crate::error::{HostingError, HostingResult};

/// Parámetros de red. Los valores por defecto son los de producción.
#[derive(Debug, Clone)]
pub struct HttpConfig {
    /// Tiempo total por petición.
    pub timeout: Duration,
    pub connect_timeout: Duration,
    /// Tope de bytes de una respuesta correcta (más allá: [`HostingError::BadResponse`]).
    pub max_response_bytes: usize,
    /// Reintentos adicionales (no cuenta el primer intento).
    pub max_retries: u32,
    /// Si el servidor pide esperar más que esto, no se espera: se devuelve `RATE_LIMITED`.
    pub max_retry_wait: Duration,
    /// Espera base del retroceso exponencial entre reintentos sin `Retry-After`.
    pub backoff_base: Duration,
    /// Máximo de páginas que se recorren al juntar listas (commits, archivos, hilos).
    pub max_pages: u32,
    /// Solo pruebas: permite `http://` a 127.0.0.1/localhost y desactiva el proxy del sistema.
    pub allow_insecure_loopback: bool,
}

impl Default for HttpConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            max_response_bytes: 8 * 1024 * 1024,
            max_retries: 2,
            max_retry_wait: Duration::from_secs(10),
            backoff_base: Duration::from_millis(400),
            max_pages: 10,
            allow_insecure_loopback: false,
        }
    }
}

/// Tope de lo que se lee de una respuesta de error (solo se usa para el motivo).
const ERROR_BODY_CAP: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    scheme: String,
    host: String,
    port: u16,
}

impl Origin {
    fn of(url: &Url) -> Option<Self> {
        Some(Self {
            scheme: url.scheme().to_owned(),
            host: url.host_str()?.to_ascii_lowercase(),
            port: url.port_or_known_default()?,
        })
    }
}

/// Respuesta ya leída (con tope).
#[derive(Debug)]
pub(crate) struct Reply {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.get(name).and_then(|v| v.to_str().ok())
    }

    pub fn json<T: serde::de::DeserializeOwned>(&self) -> HostingResult<T> {
        serde_json::from_slice(&self.body).map_err(|_| {
            HostingError::bad("La respuesta del servidor no tiene el formato esperado.")
        })
    }
}

/// `Retry-After` en segundos (la forma de fecha HTTP se ignora).
pub(crate) fn retry_after(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

/// Credencial y redactor de **una** operación. Se suelta (y se borra) al terminar.
pub(crate) struct Op {
    pub auth: AuthMethod,
    pub scrub: Scrubber,
}

impl Op {
    pub fn new(auth: AuthMethod) -> Self {
        let scrub = auth.scrubber();
        Self { auth, scrub }
    }
}

/// Convierte una respuesta no exitosa en el error tipado del proveedor.
pub(crate) type Classify<'a> = &'a (dyn Fn(&Reply, &Scrubber) -> HostingError + Send + Sync);

pub(crate) struct Http {
    client: reqwest::Client,
    cfg: HttpConfig,
    origin: Origin,
}

static TLS_PROVIDER: Once = Once::new();

impl Http {
    pub fn new(base: &Url, cfg: HttpConfig) -> HostingResult<Self> {
        TLS_PROVIDER.call_once(|| {
            // Si otro componente ya instaló un proveedor, se respeta el suyo.
            let _ = rustls::crypto::ring::default_provider().install_default();
        });
        let origin = Origin::of(base)
            .ok_or_else(|| HostingError::invalid("La dirección del proveedor no es válida."))?;
        let allowed = Self::scheme_allowed(base, &cfg);
        if !allowed {
            return Err(HostingError::invalid(
                "Solo se admiten direcciones https del proveedor.",
            ));
        }
        let policy_origin = origin.clone();
        let redirect = reqwest::redirect::Policy::custom(move |attempt| {
            if attempt.previous().len() >= 3 {
                return attempt.error("demasiadas redirecciones");
            }
            match Origin::of(attempt.url()) {
                Some(o) if o == policy_origin => attempt.follow(),
                // Redirección a otro origen (p. ej. la página de inicio de sesión de Azure
                // DevOps): se detiene y el proveedor la interpreta.
                _ => attempt.stop(),
            }
        });
        let mut builder = reqwest::Client::builder()
            .timeout(cfg.timeout)
            .connect_timeout(cfg.connect_timeout)
            .redirect(redirect)
            .user_agent(concat!("Ariadna/", env!("CARGO_PKG_VERSION")));
        if cfg.allow_insecure_loopback {
            builder = builder.no_proxy();
        }
        let client = builder
            .build()
            .map_err(|_| HostingError::network("No se pudo preparar la conexión segura."))?;
        Ok(Self {
            client,
            cfg,
            origin,
        })
    }

    fn scheme_allowed(url: &Url, cfg: &HttpConfig) -> bool {
        match url.scheme() {
            "https" => true,
            "http" => {
                cfg.allow_insecure_loopback
                    && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
            }
            _ => false,
        }
    }

    pub fn config(&self) -> &HttpConfig {
        &self.cfg
    }

    /// El URL pertenece al origen del proveedor (esquema + host + puerto).
    pub fn check_url(&self, url: &Url) -> HostingResult<()> {
        if Self::scheme_allowed(url, &self.cfg) && Origin::of(url).as_ref() == Some(&self.origin) {
            Ok(())
        } else {
            Err(HostingError::bad(
                "El servidor apuntó a una dirección fuera del proveedor; se ignora.",
            ))
        }
    }

    /// Una petición con reintentos acotados. Devuelve la respuesta **exitosa** (2xx) o el
    /// error tipado que produzca `classify`.
    pub async fn send(
        &self,
        op: &Op,
        method: Method,
        url: Url,
        body: Option<&serde_json::Value>,
        headers: &[(&str, &str)],
        classify: Classify<'_>,
    ) -> HostingResult<Reply> {
        self.check_url(&url)?;
        let idempotent = method == Method::GET;
        let payload = body
            .map(|b| serde_json::to_vec(b).map_err(|_| HostingError::invalid("Cuerpo no válido.")))
            .transpose()?;
        let mut attempt: u32 = 0;
        loop {
            let outcome = self
                .once(op, &method, &url, payload.as_deref(), headers)
                .await;
            let wait = match outcome {
                Ok(reply)
                    if reply.status.is_success()
                        && reply.status != StatusCode::NON_AUTHORITATIVE_INFORMATION =>
                {
                    return Ok(reply)
                }
                Ok(reply) => {
                    let err = classify(&reply, &op.scrub);
                    let may_retry = attempt < self.cfg.max_retries;
                    match &err {
                        HostingError::RateLimited {
                            retry_after_secs, ..
                        } if may_retry => {
                            let w = retry_after_secs
                                .map(Duration::from_secs)
                                .unwrap_or_else(|| self.backoff(attempt));
                            if w > self.cfg.max_retry_wait {
                                return Err(err);
                            }
                            w
                        }
                        HostingError::Network { .. }
                            if may_retry
                                && idempotent
                                && matches!(reply.status.as_u16(), 502..=504) =>
                        {
                            self.backoff(attempt)
                        }
                        _ => return Err(err),
                    }
                }
                Err(err) => {
                    if idempotent && attempt < self.cfg.max_retries {
                        self.backoff(attempt)
                    } else {
                        return Err(err);
                    }
                }
            };
            tokio::time::sleep(wait).await;
            attempt += 1;
        }
    }

    fn backoff(&self, attempt: u32) -> Duration {
        self.cfg.backoff_base * 2u32.saturating_pow(attempt)
    }

    async fn once(
        &self,
        op: &Op,
        method: &Method,
        url: &Url,
        payload: Option<&[u8]>,
        headers: &[(&str, &str)],
    ) -> HostingResult<Reply> {
        let mut rb = self.client.request(method.clone(), url.clone());
        if !headers
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case("accept"))
        {
            rb = rb.header(ACCEPT, "application/json");
        }
        for (k, v) in headers {
            rb = rb.header(*k, *v);
        }
        if let Some(p) = payload {
            rb = rb.header(CONTENT_TYPE, "application/json").body(p.to_vec());
        }
        rb = op.auth.apply(rb);
        let resp = rb
            .send()
            .await
            .map_err(|e| self.network_error(&e, &op.scrub))?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let strict = status.is_success();
        let cap = if strict {
            self.cfg.max_response_bytes
        } else {
            ERROR_BODY_CAP
        };
        let body = self.read_capped(resp, cap, strict, &op.scrub).await?;
        Ok(Reply {
            status,
            headers,
            body,
        })
    }

    async fn read_capped(
        &self,
        mut resp: reqwest::Response,
        cap: usize,
        strict: bool,
        scrub: &Scrubber,
    ) -> HostingResult<Vec<u8>> {
        let too_big = || {
            HostingError::bad(format!(
                "La respuesta del servidor supera el límite de {} MiB y se descarta.",
                self.cfg.max_response_bytes / (1024 * 1024)
            ))
        };
        if strict && resp.content_length().is_some_and(|n| n > cap as u64) {
            return Err(too_big());
        }
        let mut buf: Vec<u8> = Vec::new();
        loop {
            match resp.chunk().await {
                Ok(Some(chunk)) => {
                    if buf.len() + chunk.len() > cap {
                        if strict {
                            return Err(too_big());
                        }
                        let room = cap - buf.len();
                        buf.extend_from_slice(&chunk[..room]);
                        break;
                    }
                    buf.extend_from_slice(&chunk);
                }
                Ok(None) => break,
                Err(e) => return Err(self.network_error(&e, scrub)),
            }
        }
        Ok(buf)
    }

    fn network_error(&self, e: &reqwest::Error, scrub: &Scrubber) -> HostingError {
        let host = &self.origin.host;
        let msg = if e.is_timeout() {
            format!("{host} no respondió a tiempo.")
        } else if e.is_connect() {
            format!("No se pudo conectar con {host}. Comprueba la red, el proxy y el certificado.")
        } else if e.is_redirect() {
            format!("{host} redirigió la petición demasiadas veces.")
        } else {
            format!("Fallo de red al hablar con {host}.")
        };
        HostingError::network(scrub.scrub(&msg))
    }
}

/// Extrae `rel="next"` de una cabecera `Link` (GitHub).
pub(crate) fn next_link(headers: &HeaderMap) -> Option<Url> {
    let link = headers.get("link")?.to_str().ok()?;
    for part in link.split(',') {
        let part = part.trim();
        if part.contains("rel=\"next\"") {
            let start = part.find('<')? + 1;
            let end = part.find('>')?;
            return Url::parse(part.get(start..end)?).ok();
        }
    }
    None
}

/// Primer `message` legible de un cuerpo JSON de error (Azure DevOps y GitHub usan ambos
/// `message`), recortado y sin saltos de línea.
pub(crate) fn error_message(body: &[u8]) -> Option<String> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let m = v.get("message")?.as_str()?;
    let one_line: String = m.split_whitespace().collect::<Vec<_>>().join(" ");
    let t: String = one_line.chars().take(300).collect();
    (!t.is_empty()).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_link_lee_la_cabecera_de_github() {
        let mut h = HeaderMap::new();
        h.insert(
            "link",
            "<https://api.github.com/x?page=2>; rel=\"next\", <https://api.github.com/x?page=9>; rel=\"last\""
                .parse()
                .unwrap_or_else(|_| unreachable!()),
        );
        assert_eq!(
            next_link(&h).map(|u| u.to_string()).as_deref(),
            Some("https://api.github.com/x?page=2")
        );
        assert!(next_link(&HeaderMap::new()).is_none());
    }

    #[test]
    fn mensaje_de_error_se_acorta() {
        let body = br#"{"message":"  Hola\n   mundo "}"#;
        assert_eq!(error_message(body).as_deref(), Some("Hola mundo"));
        assert!(error_message(b"<html>").is_none());
    }

    #[test]
    fn solo_https_salvo_loopback_de_pruebas() {
        let cfg = HttpConfig::default();
        let u = Url::parse("http://example.com").unwrap_or_else(|_| unreachable!());
        assert!(Http::new(&u, cfg.clone()).is_err());
        let l = Url::parse("http://127.0.0.1:1").unwrap_or_else(|_| unreachable!());
        assert!(Http::new(&l, cfg).is_err());
        let ok = HttpConfig {
            allow_insecure_loopback: true,
            ..HttpConfig::default()
        };
        assert!(Http::new(&l, ok).is_ok());
    }
}
