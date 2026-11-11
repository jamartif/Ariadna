//! Ayudas comunes de las pruebas con servidor simulado.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used)]

use std::time::Duration;

use ariadna_hosting::HttpConfig;
use serde_json::Value;
use url::Url;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

pub const USER: &str = "tester";
/// Token de prueba reconocible: si aparece en cualquier salida, la prueba falla.
pub const TOKEN: &str = "ghp_TESTTOKEN_0123456789abcdef";
/// `base64("tester:ghp_TESTTOKEN_0123456789abcdef")`.
pub const BASIC: &str = "dGVzdGVyOmdocF9URVNUVE9LRU5fMDEyMzQ1Njc4OWFiY2RlZg==";

/// Red rápida: sin proxy, `http` a loopback y esperas mínimas.
pub fn fast() -> HttpConfig {
    HttpConfig {
        allow_insecure_loopback: true,
        backoff_base: Duration::from_millis(1),
        max_retry_wait: Duration::from_secs(3),
        timeout: Duration::from_secs(5),
        ..HttpConfig::default()
    }
}

pub fn base_url(server: &MockServer, suffix: &str) -> Url {
    Url::parse(&format!("{}{suffix}", server.uri())).unwrap()
}

pub async fn ok(server: &MockServer, m: &str, p: &str, body: Value) {
    Mock::given(method(m))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(server)
        .await;
}

pub async fn err(server: &MockServer, m: &str, p: &str, status: u16, body: Value) {
    Mock::given(method(m))
        .and(path(p))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

pub fn assert_no_secret(text: &str) {
    assert!(!text.contains(TOKEN), "el token aparece en: {text}");
    assert!(
        !text.contains("TESTTOKEN"),
        "parte del token aparece en: {text}"
    );
    assert!(
        !text.contains(BASIC),
        "la cabecera Basic aparece en: {text}"
    );
}
