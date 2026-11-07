#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use renault_api::{Auth, AuthOutcome, DeviceIds, RenaultError};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string_contains, method, path, query_param},
};

#[tokio::test]
async fn password_login_exports_only_a_reusable_session() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "POST",
        "/accounts.login",
        fixture("gigya/login.json"),
    )
    .await;
    let api = client(&server);
    assert_eq!(
        api.authenticate(credentials()).await.unwrap(),
        AuthOutcome::Authenticated
    );
    let session = api.session_credentials().await.unwrap();
    assert_eq!(session.login_token, "sample-cookie-value");
    assert!(
        !serde_json::to_string(&session)
            .unwrap()
            .contains("password")
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_credentials_are_typed_even_on_http_200() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "POST",
        "/accounts.login",
        fixture("gigya/error/login.403042.json"),
    )
    .await;
    let api = client(&server);
    assert!(matches!(
        api.authenticate(credentials()).await,
        Err(RenaultError::InvalidCredentials)
    ));
    assert!(api.session_credentials().await.is_none());
}

#[tokio::test]
async fn full_email_two_factor_flow_and_session_restore() {
    let server = MockServer::start().await;
    setup_tfa(&server, fixture("gigya/tfa_get_emails.json")).await;
    let api = client(&server);
    assert_eq!(
        api.authenticate(credentials()).await.unwrap(),
        AuthOutcome::TwoFactorRequired
    );
    assert!(matches!(
        api.accounts().await,
        Err(RenaultError::NotAuthenticated)
    ));
    let emails = api.two_factor_emails().await.unwrap();
    assert_eq!(emails[0].obfuscated.as_deref(), Some("e***@example.com"));
    api.request_two_factor_code(None).await.unwrap();
    assert_eq!(
        api.complete_two_factor("123456", true).await.unwrap(),
        AuthOutcome::Authenticated
    );
    assert!(matches!(
        api.complete_two_factor("123456", true).await,
        Err(RenaultError::NoPendingTwoFactor)
    ));
    let session = api.session_credentials().await.unwrap();
    assert_eq!(session.device_ids.gmid.as_deref(), Some("sample-gmid"));
    assert_eq!(session.device_ids.ucid.as_deref(), Some("sample-ucid"));

    mock_jwt(&server, &jwt(), 1).await;
    let restored = client(&server);
    restored
        .authenticate(Auth::RestoredSession(session.clone()))
        .await
        .unwrap();
    assert_eq!(restored.device_ids().await, session.device_ids);

    let requests = server.received_requests().await.unwrap();
    for request in requests
        .iter()
        .filter(|r| r.url.path().starts_with("/accounts.tfa."))
    {
        assert_eq!(
            request.headers.get("cookie").unwrap().to_str().unwrap(),
            "gmid=sample-gmid; ucid=sample-ucid"
        );
    }
    let finalize = requests
        .iter()
        .find(|r| r.url.path() == "/accounts.tfa.finalizeTFA")
        .unwrap();
    assert!(
        finalize
            .url
            .query_pairs()
            .any(|(k, v)| k == "tempDevice" && v == "false")
    );
}

#[tokio::test]
async fn wrong_code_can_be_retried_without_requesting_another() {
    let server = MockServer::start().await;
    setup_tfa(&server, fixture("gigya/tfa_get_emails.json")).await;
    Mock::given(method("GET"))
        .and(path("/accounts.tfa.email.completeVerification"))
        .and(query_param("code", "wrong"))
        .respond_with(ResponseTemplate::new(400).set_body_json(fixture(
            "gigya/error/tfa_complete_email_verification.400006.json",
        )))
        .expect(1)
        .mount(&server)
        .await;
    let api = client(&server);
    api.authenticate(credentials()).await.unwrap();
    api.request_two_factor_code(None).await.unwrap();
    assert!(matches!(
        api.complete_two_factor("wrong", true).await,
        Err(RenaultError::InvalidVerificationCode)
    ));
    assert_eq!(
        api.complete_two_factor("123456", true).await.unwrap(),
        AuthOutcome::Authenticated
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.url.path() == "/accounts.tfa.email.sendVerificationCode")
            .count(),
        1
    );
}

#[tokio::test]
async fn several_emails_require_an_explicit_selection() {
    let server = MockServer::start().await;
    setup_tfa(&server, fixture("gigya/tfa_get_emails.multiple.json")).await;
    let api = client(&server);
    api.authenticate(credentials()).await.unwrap();
    assert!(matches!(
        api.request_two_factor_code(None).await,
        Err(RenaultError::EmailSelectionRequired)
    ));
    assert!(matches!(
        api.request_two_factor_code(Some("unknown")).await,
        Err(RenaultError::InvalidInput(_))
    ));
    let emails = api.two_factor_emails().await.unwrap();
    api.request_two_factor_code(Some(&emails[1].id))
        .await
        .unwrap();
    api.complete_two_factor("123456", false).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().any(|r| {
        r.url.path() == "/accounts.tfa.finalizeTFA"
            && r.url
                .query_pairs()
                .any(|(k, v)| k == "tempDevice" && v == "true")
    }));
}

#[tokio::test]
async fn finalization_retry_does_not_submit_a_verified_otp_again() {
    let server = MockServer::start().await;
    setup_tfa(&server, fixture("gigya/tfa_get_emails.json")).await;
    let api = client(&server);
    api.authenticate(credentials()).await.unwrap();
    api.request_two_factor_code(None).await.unwrap();
    Mock::given(method("GET"))
        .and(path("/accounts.tfa.finalizeTFA"))
        .respond_with(ResponseTemplate::new(503).set_body_string("service unavailable"))
        .with_priority(1)
        .up_to_n_times(1)
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        api.complete_two_factor("123456", true).await,
        Err(RenaultError::Http { status: 503, .. })
    ));
    assert_eq!(
        api.complete_two_factor("already-verified", true)
            .await
            .unwrap(),
        AuthOutcome::Authenticated
    );
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.url.path() == "/accounts.tfa.email.completeVerification")
            .count(),
        1
    );
}

#[tokio::test]
async fn no_registered_email_is_a_typed_error() {
    let server = MockServer::start().await;
    setup_tfa(&server, fixture("gigya/tfa_get_emails.empty.json")).await;
    let api = client(&server);
    api.authenticate(credentials()).await.unwrap();
    assert!(matches!(
        api.request_two_factor_code(None).await,
        Err(RenaultError::NoTwoFactorEmail)
    ));
}

#[tokio::test]
async fn challenge_requires_a_requested_code_and_can_be_cancelled() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "POST",
        "/accounts.login",
        fixture("gigya/error/login.403101.json"),
    )
    .await;
    let api = client(&server);
    assert!(matches!(
        api.two_factor_emails().await,
        Err(RenaultError::NoPendingTwoFactor)
    ));
    api.authenticate(credentials()).await.unwrap();
    assert!(matches!(
        api.complete_two_factor("123456", true).await,
        Err(RenaultError::VerificationCodeNotRequested)
    ));
    api.cancel_two_factor().await;
    assert!(matches!(
        api.request_two_factor_code(None).await,
        Err(RenaultError::NoPendingTwoFactor)
    ));
}

#[tokio::test]
async fn remembered_device_ids_are_sent_on_the_first_login() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/accounts.login"))
        .and(wiremock::matchers::header(
            "cookie",
            "gmid=remembered; ucid=device",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(fixture("gigya/login.json")))
        .expect(1)
        .mount(&server)
        .await;
    let api = client(&server);
    api.set_device_ids(DeviceIds {
        gmid: Some("remembered".into()),
        ucid: Some("device".into()),
    })
    .await
    .unwrap();
    api.authenticate(credentials()).await.unwrap();
}

#[tokio::test]
async fn revoked_session_clears_credentials() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "POST",
        "/accounts.getJWT",
        fixture("gigya/error/get_jwt.403013.json"),
    )
    .await;
    let api = client(&server);
    assert!(matches!(
        api.authenticate(Auth::SessionToken("revoked".into())).await,
        Err(RenaultError::AuthenticationExpired)
    ));
    assert!(api.session_credentials().await.is_none());
}

#[tokio::test]
async fn direct_jwt_does_not_require_a_gigya_request() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "GET",
        "/commerce/v1/persons/person-id-1",
        fixture("kamereon/person.json"),
    )
    .await;
    let api = authenticated(&server).await;
    assert!(!api.accounts().await.unwrap().is_empty());
    assert!(api.session_credentials().await.is_none());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_and_expired_jwts_are_rejected() {
    let server = MockServer::start().await;
    let api = client(&server);
    for token in ["garbage".into(), jwt_with(json!({"data": {}}))] {
        assert!(matches!(
            api.authenticate(Auth::AccessToken(token)).await,
            Err(RenaultError::InvalidAccessToken)
        ));
    }
    for exp in [now() - 1, now() + 30] {
        assert!(matches!(
            api.authenticate(Auth::AccessToken(jwt_with(json!({"exp": exp}))))
                .await,
            Err(RenaultError::AuthenticationExpired)
        ));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn session_falls_back_to_account_info_when_jwt_has_no_person_id() {
    let server = MockServer::start().await;
    mock_jwt(&server, &jwt_with(json!({"exp": now() + 3600})), 1).await;
    Mock::given(method("POST"))
        .and(path("/accounts.getAccountInfo"))
        .and(body_string_contains("login_token=session"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(fixture("gigya/get_account_info.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    mock_json(
        &server,
        "GET",
        "/commerce/v1/persons/person-id-1",
        fixture("kamereon/person.json"),
    )
    .await;
    let api = client(&server);
    api.authenticate(Auth::SessionToken("session".into()))
        .await
        .unwrap();
    api.accounts().await.unwrap();
    api.accounts().await.unwrap();
}

#[tokio::test]
async fn jwt_without_person_id_can_still_use_explicit_account_calls() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "GET",
        &data_path("cockpit", 1),
        json!({"data":{"attributes":{"totalMileage": 12.5}}}),
    )
    .await;
    let api = client(&server);
    api.authenticate(Auth::AccessToken(jwt_with(json!({"exp": now() + 3600}))))
        .await
        .unwrap();
    assert!(matches!(
        api.accounts().await,
        Err(RenaultError::InvalidInput(_))
    ));
    assert_eq!(
        api.cockpit(ACCOUNT, VIN).await.unwrap().total_mileage_km,
        Some(12.5)
    );
}

#[tokio::test]
async fn concurrent_calls_mint_only_one_jwt_and_share_logout() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "POST",
        "/accounts.login",
        fixture("gigya/login.json"),
    )
    .await;
    mock_jwt(&server, &jwt(), 1).await;
    mock_json(
        &server,
        "GET",
        &data_path("cockpit", 1),
        json!({"data":{"attributes":{}}}),
    )
    .await;
    let api = client(&server);
    api.authenticate(credentials()).await.unwrap();
    let mut tasks = Vec::new();
    for _ in 0..20 {
        let api = api.clone();
        tasks.push(tokio::spawn(async move { api.cockpit(ACCOUNT, VIN).await }));
    }
    for task in tasks {
        task.await.unwrap().unwrap();
    }
    let other = api.clone();
    api.logout().await;
    assert!(other.session_credentials().await.is_none());
    assert!(matches!(
        other.cockpit(ACCOUNT, VIN).await,
        Err(RenaultError::NotAuthenticated)
    ));
}

#[tokio::test]
async fn a_rejected_jwt_is_refreshed_on_the_next_call_without_retrying_the_failed_call() {
    let server = MockServer::start().await;
    mock_jwt(&server, &jwt(), 2).await;
    let api = client(&server);
    api.authenticate(Auth::SessionToken("session".into()))
        .await
        .unwrap();
    let rejection = Mock::given(method("GET"))
        .and(path(data_path("cockpit", 1)))
        .respond_with(ResponseTemplate::new(401).set_body_string("expired"))
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    assert!(matches!(
        api.cockpit(ACCOUNT, VIN).await,
        Err(RenaultError::AuthenticationExpired)
    ));
    drop(rejection);
    mock_json(
        &server,
        "GET",
        &data_path("cockpit", 1),
        json!({"data":{"attributes":{}}}),
    )
    .await;
    api.cockpit(ACCOUNT, VIN).await.unwrap();
}

#[tokio::test]
async fn debug_and_transport_errors_do_not_expose_secrets() {
    let secret = "UNIQUE-SECRET";
    assert!(
        !format!(
            "{:?}",
            Auth::Credentials {
                email: secret.into(),
                password: secret.into()
            }
        )
        .contains(secret)
    );
    assert!(!format!("{:?}", Auth::SessionToken(secret.into())).contains(secret));
    let server = MockServer::start().await;
    setup_tfa(&server, fixture("gigya/tfa_get_emails.json")).await;
    let endpoints =
        renault_api::Endpoints::new(&server.uri(), "gigya-key", &server.uri(), "kamereon-key")
            .unwrap();
    let config = renault_api::Config::with_endpoints("fr_FR", "FR", endpoints)
        .unwrap()
        .with_timeout(std::time::Duration::from_millis(200))
        .unwrap();
    let api = renault_api::RenaultApi::new(config).unwrap();
    api.authenticate(credentials()).await.unwrap();
    api.request_two_factor_code(None).await.unwrap();
    let failure = Mock::given(method("GET"))
        .and(path("/accounts.tfa.email.completeVerification"))
        .and(query_param("code", secret))
        .respond_with(ResponseTemplate::new(200).set_body_string("PRIVATE BODY"))
        .expect(1)
        .mount_as_scoped(&server)
        .await;
    let error = api.complete_two_factor(secret, true).await.unwrap_err();
    assert!(!format!("{error:?}").contains(secret));
    assert!(!format!("{error:?}").contains("PRIVATE BODY"));
    drop(failure);
    // A timeout while reading a TFA URL must strip assertions and OTP query values.
    let delayed = Mock::given(method("GET"))
        .and(path("/accounts.tfa.email.completeVerification"))
        .and(query_param("code", secret))
        .respond_with(ResponseTemplate::new(200).set_delay(std::time::Duration::from_secs(1)))
        .mount_as_scoped(&server)
        .await;
    let error = api.complete_two_factor(secret, true).await.unwrap_err();
    assert!(matches!(error, RenaultError::Transport(_)));
    assert!(!format!("{error:?}").contains(secret));
    drop(delayed);
}
