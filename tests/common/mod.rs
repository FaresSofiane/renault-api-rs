#![allow(dead_code)]

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use renault_api::{Auth, Config, Endpoints, RenaultApi};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path, query_param},
};

pub const ACCOUNT: &str = "account-id-1";
pub const VIN: &str = "VF1AAAAA555777999";

pub fn fixture(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

pub fn jwt_with(claims: Value) -> String {
    format!(
        "{}.{}.signature",
        URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#),
        URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
    )
}

pub fn jwt() -> String {
    jwt_with(json!({"exp": now() + 3600, "data": {"personId": "person-id-1"}}))
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub fn client(server: &MockServer) -> RenaultApi {
    let endpoints =
        Endpoints::new(&server.uri(), "gigya-key", &server.uri(), "kamereon-key").unwrap();
    RenaultApi::new(Config::with_endpoints("fr_FR", "FR", endpoints).unwrap()).unwrap()
}

pub async fn authenticated(server: &MockServer) -> RenaultApi {
    let api = client(server);
    api.authenticate(Auth::AccessToken(jwt())).await.unwrap();
    api
}

pub async fn mock_json(server: &MockServer, verb: &str, route: &str, value: Value) {
    Mock::given(method(verb))
        .and(path(route))
        .respond_with(ResponseTemplate::new(200).set_body_json(value))
        .mount(server)
        .await;
}

pub async fn mock_jwt(server: &MockServer, token: &str, count: u64) {
    Mock::given(method("POST"))
        .and(path("/accounts.getJWT"))
        .and(body_string_contains("ApiKey=gigya-key"))
        .and(body_string_contains("expiration=900"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"errorCode": 0, "id_token": token})),
        )
        .expect(count)
        .mount(server)
        .await;
}

pub async fn mock_details(server: &MockServer, model: &str) {
    mock_json(
        server,
        "GET",
        &format!("/commerce/v1/accounts/{ACCOUNT}/vehicles/{VIN}/details"),
        json!({"vin": VIN, "model": {"code": model}}),
    )
    .await;
}

pub fn data_path(endpoint: &str, version: u8) -> String {
    format!(
        "/commerce/v1/accounts/{ACCOUNT}/kamereon/kca/car-adapter/v{version}/cars/{VIN}/{endpoint}"
    )
}

pub async fn setup_tfa(server: &MockServer, emails: Value) {
    // The first login has no device cookies; the post-TFA login must have them.
    Mock::given(method("POST"))
        .and(path("/accounts.login"))
        .and(body_string_contains("loginID=email%40example.com"))
        .and(body_string_contains("password=password"))
        .respond_with(|request: &wiremock::Request| {
            if request.headers.contains_key("cookie") {
                ResponseTemplate::new(200).set_body_json(fixture("gigya/login.json"))
            } else {
                ResponseTemplate::new(200).set_body_json(fixture("gigya/error/login.403101.json"))
            }
        })
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/accounts.webSdkBootstrap"))
        .and(query_param("APIKey", "gigya-key"))
        .respond_with(
            ResponseTemplate::new(200)
                .append_header("set-cookie", "gmid=sample-gmid; Path=/; HttpOnly")
                .append_header("set-cookie", "ucid=sample-ucid; Path=/; HttpOnly"),
        )
        .expect(1)
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/accounts.tfa.initTFA"))
        .and(query_param("regToken", "sample-reg-token"))
        .and(query_param("provider", "gigyaEmail"))
        .and(query_param("mode", "verify"))
        .and(query_param("gmid", "sample-gmid"))
        .and(query_param("ucid", "sample-ucid"))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("gigya/tfa_init.json")))
        .expect(1)
        .mount(server)
        .await;
    mock_json(server, "GET", "/accounts.tfa.email.getEmails", emails).await;
    Mock::given(method("GET"))
        .and(path("/accounts.tfa.email.sendVerificationCode"))
        .and(query_param("lang", "fr"))
        .and(query_param("gigyaAssertion", "sample-gigya-assertion"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("gigya/tfa_send_email_code.json")),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/accounts.tfa.email.completeVerification"))
        .and(query_param("code", "123456"))
        .and(query_param("phvToken", "sample-phv-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(fixture("gigya/tfa_complete_email_verification.json")),
        )
        .mount(server)
        .await;
    Mock::given(method("GET"))
        .and(path("/accounts.tfa.finalizeTFA"))
        .and(query_param(
            "providerAssertion",
            "sample-provider-assertion",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("gigya/tfa_finalize_tfa.json")),
        )
        .mount(server)
        .await;
}

pub fn credentials() -> Auth {
    Auth::Credentials {
        email: "email@example.com".into(),
        password: "password".into(),
    }
}
