#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::*;
use renault_api::{
    ApiErrorKind, RenaultError, Service,
    models::{ChargingStatus, PlugStatus},
};
use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path, query_param},
};

#[tokio::test]
async fn vehicle_list_and_details_decode_real_python_fixtures() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "GET",
        &format!("/commerce/v1/accounts/{ACCOUNT}/vehicles"),
        fixture("kamereon/vehicles/zoe_40.1.json"),
    )
    .await;
    mock_json(
        &server,
        "GET",
        &format!("/commerce/v1/accounts/{ACCOUNT}/vehicles/{VIN}/details"),
        fixture("kamereon/vehicle_details/zoe_40.1.json"),
    )
    .await;
    let api = authenticated(&server).await;
    let vehicles = api.vehicles(ACCOUNT).await.unwrap();
    assert_eq!(vehicles[0].vin, VIN);
    let details = api.vehicle_details(ACCOUNT, VIN).await.unwrap();
    assert_eq!(details.model_code(), Some("X101VE"));
    assert!(details.picture_url().unwrap().starts_with("https://"));
}

#[tokio::test]
async fn every_vehicle_list_fixture_is_supported() {
    let server = MockServer::start().await;
    let api = authenticated(&server).await;
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/kamereon/vehicles");
    let mut count = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let file = entry.unwrap().path();
        if file.extension().and_then(|v| v.to_str()) != Some("json") {
            continue;
        }
        let response: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let expected = response["vehicleLinks"].as_array().unwrap().len();
        let scoped = Mock::given(method("GET"))
            .and(path(format!("/commerce/v1/accounts/{ACCOUNT}/vehicles")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        let result = api
            .vehicles(ACCOUNT)
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert_eq!(result.len(), expected, "{}", file.display());
        drop(scoped);
        count += 1;
    }
    assert!(count > 0, "the fixture directory must not be empty");
}

#[tokio::test]
async fn battery_normalizes_zoe_40_watts_and_preserves_unknown_values() {
    let server = MockServer::start().await;
    mock_details(&server, "X101VE").await;
    let mut battery = fixture("kamereon/vehicle_data/battery-status.2.json");
    battery["data"]["attributes"]["chargingInstantaneousPower"] = json!(27000.0);
    battery["data"]["attributes"]["chargingStatus"] = json!(9.9);
    battery["data"]["attributes"]["plugStatus"] = json!(42);
    mock_json(&server, "GET", &data_path("battery-status", 2), battery).await;
    let api = authenticated(&server).await;
    let battery = api.battery_status(ACCOUNT, VIN).await.unwrap();
    assert_eq!(battery.charging_power_kw, Some(27.0));
    assert_eq!(battery.charging_status, Some(ChargingStatus::Unknown(9.9)));
    assert_eq!(battery.plug_status, Some(PlugStatus::Unknown(42)));
    assert_eq!(battery.charging_remaining_minutes, Some(145));
    assert!(battery.updated_at.is_some());
}

#[tokio::test]
async fn every_copied_telemetry_fixture_decodes_through_the_public_client() {
    let server = MockServer::start().await;
    mock_details(&server, "X102VE").await;
    let api = authenticated(&server).await;
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/kamereon/vehicle_data");
    let mut count = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let file = entry.unwrap().path();
        let name = file.file_name().unwrap().to_str().unwrap();
        let endpoint = name.split('.').next().unwrap();
        let version = if endpoint == "battery-status" { 2 } else { 1 };
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let scoped = Mock::given(method("GET"))
            .and(path(data_path(endpoint, version)))
            .respond_with(ResponseTemplate::new(200).set_body_json(value))
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        let result = match endpoint {
            "battery-status" => api.battery_status(ACCOUNT, VIN).await.map(|_| ()),
            "cockpit" => api.cockpit(ACCOUNT, VIN).await.map(|_| ()),
            "location" => api.location(ACCOUNT, VIN).await.map(|_| ()),
            _ => panic!("unexpected telemetry fixture {name}"),
        };
        result.unwrap_or_else(|error| panic!("{name}: {error}"));
        drop(scoped);
        count += 1;
    }
    assert!(count > 0);
}

#[tokio::test]
async fn modern_battery_keeps_kw_and_independent_timestamps() {
    let server = MockServer::start().await;
    mock_details(&server, "R5E1VE").await;
    mock_json(
        &server,
        "GET",
        &data_path("battery-status", 2),
        fixture("kamereon/vehicle_data/battery-status.renault_5_2.json"),
    )
    .await;
    let api = authenticated(&server).await;
    let battery = api.battery_status(ACCOUNT, VIN).await.unwrap();
    assert_eq!(battery.level_percent, Some(73));
    assert_eq!(battery.range_km, Some(261.0));
    assert_eq!(battery.temperature_celsius, None);
    assert_eq!(battery.charging_power_kw, None);
    assert_eq!(battery.charging_status, Some(ChargingStatus::NotCharging));
    assert_ne!(battery.updated_at, battery.charging_remaining_updated_at);
}

#[tokio::test]
async fn known_unsupported_endpoints_do_not_send_telemetry_requests() {
    let server = MockServer::start().await;
    let zoe = Mock::given(method("GET"))
        .and(path(format!(
            "/commerce/v1/accounts/{ACCOUNT}/vehicles/{VIN}/details"
        )))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"model":{"code":"X101VE"}})))
        .mount_as_scoped(&server)
        .await;
    let api = authenticated(&server).await;
    assert!(matches!(
        api.location(ACCOUNT, VIN).await,
        Err(RenaultError::UnsupportedEndpoint {
            endpoint: "location",
            ..
        })
    ));
    drop(zoe);
    mock_details(&server, "XHN1ML").await;
    // Refresh details explicitly to change the model used for routing.
    api.vehicle_details(ACCOUNT, VIN).await.unwrap();
    assert!(matches!(
        api.battery_status(ACCOUNT, VIN).await,
        Err(RenaultError::UnsupportedEndpoint {
            endpoint: "battery-status",
            ..
        })
    ));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.url.path().ends_with("/details"))
    );
}

#[tokio::test]
async fn list_metadata_avoids_an_extra_details_request() {
    let server = MockServer::start().await;
    mock_json(
        &server,
        "GET",
        &format!("/commerce/v1/accounts/{ACCOUNT}/vehicles"),
        fixture("kamereon/vehicles/zoe_40.1.json"),
    )
    .await;
    mock_json(
        &server,
        "GET",
        &data_path("battery-status", 2),
        fixture("kamereon/vehicle_data/battery-status.1.json"),
    )
    .await;
    let api = authenticated(&server).await;
    api.vehicles(ACCOUNT).await.unwrap();
    api.battery_status(ACCOUNT, VIN).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn cockpit_has_no_artificial_zero_values_and_location_has_typed_time() {
    let server = MockServer::start().await;
    mock_details(&server, "X102VE").await;
    mock_json(
        &server,
        "GET",
        &data_path("cockpit", 1),
        json!({"data":{"attributes":{"totalMileage":1234.5, "fuelQuantity":null}}}),
    )
    .await;
    mock_json(
        &server,
        "GET",
        &data_path("location", 1),
        fixture("kamereon/vehicle_data/location.2.json"),
    )
    .await;
    let api = authenticated(&server).await;
    let cockpit = api.cockpit(ACCOUNT, VIN).await.unwrap();
    assert_eq!(cockpit.total_mileage_km, Some(1234.5));
    assert_eq!(cockpit.fuel_quantity_liters, None);
    assert_eq!(cockpit.fuel_range_km, None);
    let location = api.location(ACCOUNT, VIN).await.unwrap();
    assert_eq!(location.latitude, Some(48.1234567));
    assert_eq!(
        location.updated_at.unwrap().to_rfc3339(),
        "2020-02-18T16:58:38+00:00"
    );
}

#[tokio::test]
async fn requests_use_renault_headers_country_and_correct_versions() {
    let server = MockServer::start().await;
    let token = jwt();
    Mock::given(method("GET"))
        .and(path(data_path("cockpit", 1)))
        .and(header("apikey", "kamereon-key"))
        .and(header("x-gigya-id_token", &token))
        .and(header("content-type", "application/vnd.api+json"))
        .and(query_param("country", "FR"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data":{"attributes":{}}})))
        .expect(1)
        .mount(&server)
        .await;
    let api = client(&server);
    api.authenticate(renault_api::Auth::AccessToken(token))
        .await
        .unwrap();
    api.cockpit(ACCOUNT, VIN).await.unwrap();
}

#[tokio::test]
async fn missing_envelopes_invalid_dates_and_out_of_range_values_are_errors() {
    let server = MockServer::start().await;
    mock_details(&server, "X102VE").await;
    let api = authenticated(&server).await;
    for attributes in [
        json!({"batteryLevel":101}),
        json!({"timestamp":"not-a-date"}),
        json!({"batteryLevel":-1}),
    ] {
        let scoped = Mock::given(method("GET"))
            .and(path(data_path("battery-status", 2)))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data":{"attributes":attributes}})),
            )
            .mount_as_scoped(&server)
            .await;
        assert!(matches!(
            api.battery_status(ACCOUNT, VIN).await,
            Err(RenaultError::InvalidResponse { .. })
        ));
        drop(scoped);
    }
    let missing = Mock::given(method("GET"))
        .and(path(data_path("cockpit", 1)))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
        .mount_as_scoped(&server)
        .await;
    assert!(matches!(
        api.cockpit(ACCOUNT, VIN).await,
        Err(RenaultError::InvalidResponse { .. })
    ));
    drop(missing);
    mock_json(
        &server,
        "GET",
        &data_path("location", 1),
        json!({"data":{"attributes":{"gpsLatitude":100}}}),
    )
    .await;
    assert!(matches!(
        api.location(ACCOUNT, VIN).await,
        Err(RenaultError::InvalidResponse { .. })
    ));
}

#[tokio::test]
async fn upstream_errors_are_classified_without_exposing_response_bodies() {
    let server = MockServer::start().await;
    let api = authenticated(&server).await;
    for (name, kind) in [
        ("quota_limit", ApiErrorKind::QuotaExceeded),
        ("not_supported", ApiErrorKind::NotSupported),
        ("resource_not_found", ApiErrorKind::NotFound),
        ("privacy_on", ApiErrorKind::PrivacyMode),
        ("access_denied", ApiErrorKind::AccessDenied),
        ("invalid_upstream", ApiErrorKind::Upstream),
    ] {
        let scoped = Mock::given(method("GET"))
            .and(path(data_path("cockpit", 1)))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(fixture(&format!("kamereon/error/{name}.json"))),
            )
            .mount_as_scoped(&server)
            .await;
        let error = api.cockpit(ACCOUNT, VIN).await.unwrap_err();
        assert!(
            matches!(error, RenaultError::Api { service: Service::Kamereon, kind: actual, .. } if actual == kind),
            "{name}: {error}"
        );
        drop(scoped);
    }
    let scoped = Mock::given(method("GET"))
        .and(path(data_path("cockpit", 1)))
        .respond_with(ResponseTemplate::new(429).set_body_json(
            json!({"errors":[{"errorCode":"future.code","errorMessage":"PRIVATE"}]}),
        ))
        .mount_as_scoped(&server)
        .await;
    let error = api.cockpit(ACCOUNT, VIN).await.unwrap_err();
    assert!(
        matches!(&error, RenaultError::Api { code, kind: ApiErrorKind::Other, status:429, .. } if code == "future.code")
    );
    assert!(!format!("{error:?}").contains("PRIVATE"));
    drop(scoped);
}

#[tokio::test]
async fn html_http_errors_redirects_and_large_bodies_are_not_silent_successes() {
    let server = MockServer::start().await;
    let api = authenticated(&server).await;
    for status in [302, 403, 429, 502] {
        let scoped = Mock::given(method("GET"))
            .and(path(data_path("cockpit", 1)))
            .respond_with(
                ResponseTemplate::new(status)
                    .insert_header("location", "/do-not-follow")
                    .set_body_string("<html>upstream error</html>"),
            )
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        assert!(
            matches!(api.cockpit(ACCOUNT, VIN).await, Err(RenaultError::Http { status:s, .. }) if s == status)
        );
        drop(scoped);
    }
    let scoped = Mock::given(method("GET"))
        .and(path(data_path("cockpit", 1)))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(2 * 1024 * 1024 + 1)))
        .mount_as_scoped(&server)
        .await;
    assert!(matches!(
        api.cockpit(ACCOUNT, VIN).await,
        Err(RenaultError::InvalidResponse { .. })
    ));
    drop(scoped);
}

#[tokio::test]
async fn malformed_identifiers_are_rejected_and_path_segments_are_encoded() {
    let server = MockServer::start().await;
    let api = authenticated(&server).await;
    for id in ["", ".", "..", "invalid\n"] {
        assert!(matches!(
            api.vehicles(id).await,
            Err(RenaultError::InvalidInput(_))
        ));
        assert!(matches!(
            api.cockpit(ACCOUNT, id).await,
            Err(RenaultError::InvalidInput(_))
        ));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    Mock::given(method("GET"))
        .and(path("/commerce/v1/accounts/a%2Fb%3Fc/vehicles"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"vehicleLinks":[]})))
        .expect(1)
        .mount(&server)
        .await;
    assert!(api.vehicles("a/b?c").await.unwrap().is_empty());
}
