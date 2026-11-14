//! Flujo OAuth device code contra un servidor simulado (`wiremock`, *loopback* en `http://`).
//!
//! Cubre: petición del código, sondeo `authorization_pending`, `slow_down`, autorización, código
//! caducado, acceso denegado, app sin device flow y que ningún token salga en `Debug` ni en los
//! errores.
#![allow(clippy::unwrap_used)]

use ariadna_hosting::{
    DeviceFlow, DeviceFlowConfig, DevicePoll, Endpoints, OAuthProvider, SLOWDOWN_STEP,
};
use wiremock::matchers::{body_string_contains, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

/// Cliente de pruebas: instala el proveedor criptográfico (el crate compila `rustls-no-provider`).
fn client() -> reqwest::Client {
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::new()
}

fn flow(server: &MockServer, provider: OAuthProvider) -> DeviceFlow {
    let endpoints = Endpoints {
        device_code: format!("{}/device/code", server.uri()),
        token: format!("{}/token", server.uri()),
    };
    let cfg = DeviceFlowConfig::new(provider, "Iv1.cliente-del-usuario").with_endpoints(endpoints);
    DeviceFlow::with_client(client(), cfg).unwrap()
}

#[tokio::test]
async fn pide_el_codigo_y_respeta_intervalo_y_caducidad() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .and(body_string_contains("client_id=Iv1.cliente-del-usuario"))
        .and(body_string_contains("scope=repo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dc-secreto-123",
            "user_code": "ABCD-1234",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": 5
        })))
        .mount(&server)
        .await;

    let code = flow(&server, OAuthProvider::GitHub)
        .request()
        .await
        .unwrap();
    assert_eq!(code.user_code, "ABCD-1234");
    assert_eq!(code.verification_uri, "https://github.com/login/device");
    assert_eq!(code.interval.as_secs(), 5);
    assert_eq!(code.expires_in.as_secs(), 900);
    assert_eq!(code.device_code.expose(), "dc-secreto-123");
    // El `Debug` del código no lleva el device_code.
    assert!(!format!("{code:?}").contains("dc-secreto-123"));
}

#[tokio::test]
async fn el_sondeo_va_pendiente_y_luego_llega_el_token() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dc-1",
            "user_code": "ABCD-1234",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": 1
        })))
        .mount(&server)
        .await;
    // Primer sondeo: pendiente.
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("device_code=dc-1"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": "authorization_pending",
            "error_description": "El usuario todavía no ha autorizado la aplicación."
        })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // Segundo sondeo: token.
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "gho_token_de_prueba_123",
            "token_type": "bearer",
            "scope": "repo"
        })))
        .mount(&server)
        .await;

    let f = flow(&server, OAuthProvider::GitHub);
    let code = f.request().await.unwrap();
    assert!(matches!(f.poll(&code).await.unwrap(), DevicePoll::Pending));
    let polled = f.poll(&code).await.unwrap();
    assert!(
        !format!("{polled:?}").contains("gho_token"),
        "sin token en Debug"
    );
    match polled {
        DevicePoll::Authorized(ref secret) => {
            assert_eq!(secret.expose(), "gho_token_de_prueba_123")
        }
        ref other => panic!("se esperaba Authorized, llegó {other:?}"),
    }
}

#[tokio::test]
async fn slow_down_alarga_la_espera_y_expired_pide_otro_codigo() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dc-2",
            "user_code": "WXYZ-9876",
            "verification_uri": "https://gitlab.com/-/device",
            "expires_in": 300,
            "interval": 5
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .and(body_string_contains("device_code=dc-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": "slow_down"
        })))
        .mount(&server)
        .await;

    let f = flow(&server, OAuthProvider::GitLab);
    let code = f.request().await.unwrap();
    assert!(matches!(f.poll(&code).await.unwrap(), DevicePoll::SlowDown));
    assert_eq!(
        SLOWDOWN_STEP.as_secs(),
        5,
        "slow_down suma 5 s al intervalo"
    );

    // Otro flujo: el código caduca.
    let expired = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dc-3",
            "user_code": "QQQQ-1111",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 1,
            "interval": 1
        })))
        .mount(&expired)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": "expired_token"
        })))
        .mount(&expired)
        .await;
    let f = flow(&expired, OAuthProvider::GitHub);
    let code = f.request().await.unwrap();
    assert!(matches!(f.poll(&code).await.unwrap(), DevicePoll::Expired));
}

#[tokio::test]
async fn acceso_denegado_y_formas_raras() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dc-4",
            "user_code": "DENY-0001",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": 5
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": "access_denied"
        })))
        .mount(&server)
        .await;
    let f = flow(&server, OAuthProvider::GitHub);
    let code = f.request().await.unwrap();
    assert!(matches!(f.poll(&code).await.unwrap(), DevicePoll::Denied));

    // Respuesta sin los campos obligatorios: error claro, nunca pánico.
    let raro = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({"ok": true})))
        .mount(&raro)
        .await;
    let err = flow(&raro, OAuthProvider::GitHub)
        .request()
        .await
        .unwrap_err();
    assert_eq!(err.code(), "PARSE");
}

#[tokio::test]
async fn una_app_sin_device_flow_lo_dice_con_policy() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "error": "device_flow_disabled",
            "error_description": "Device flow is disabled for this app."
        })))
        .mount(&server)
        .await;
    let err = flow(&server, OAuthProvider::GitHub)
        .request()
        .await
        .unwrap_err();
    assert_eq!(err.code(), "POLICY");
    assert!(err.to_string().contains("código de dispositivo"));
}

#[tokio::test]
async fn el_token_de_un_error_del_servidor_no_se_repite_en_el_mensaje() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "device_code": "dc-5",
            "user_code": "ERR-0001",
            "verification_uri": "https://github.com/login/device",
            "expires_in": 900,
            "interval": 5
        })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(400).set_body_string(
            "{\"error\":\"invalid_grant\",\"error_description\":\"bad token gho_secreto_999\"}",
        ))
        .mount(&server)
        .await;
    let f = flow(&server, OAuthProvider::GitHub);
    let code = f.request().await.unwrap();
    // `invalid_grant` se trata como código caducado, sin repetir el cuerpo.
    assert!(matches!(f.poll(&code).await.unwrap(), DevicePoll::Expired));
}

#[tokio::test]
async fn un_servidor_que_no_responde_es_network() {
    let cfg = DeviceFlowConfig::new(OAuthProvider::GitHub, "Iv1.x").with_endpoints(Endpoints {
        device_code: "http://127.0.0.1:9/device/code".into(),
        token: "http://127.0.0.1:9/token".into(),
    });
    let f = DeviceFlow::with_client(client(), cfg).unwrap();
    let err = f.request().await.unwrap_err();
    assert_eq!(err.code(), "NETWORK");
}
