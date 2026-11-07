#![allow(clippy::unwrap_used, clippy::expect_used)]
mod common;
use chrono::{NaiveDate, TimeZone, Utc};
use common::*;
use renault_api::{RenaultError, endpoints::Endpoint, models::*};
use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_json, method, path, query_param},
};

fn reference(name: &str) -> Value {
    fixture(&format!("reference/{name}"))
}
fn range() -> (NaiveDate, NaiveDate) {
    (
        NaiveDate::from_ymd_opt(2020, 11, 1).unwrap(),
        NaiveDate::from_ymd_opt(2020, 12, 31).unwrap(),
    )
}
fn route(model: &str, endpoint: Endpoint) -> String {
    let definition = endpoint.definition_for_model(Some(model)).unwrap();
    format!(
        "/commerce/v1/accounts/{ACCOUNT}/kamereon{}",
        definition.path.replace("{vin}", VIN)
    )
}

#[test]
fn complete_routing_table_matches_the_python_reference_matrix() {
    let matrix = fixture("endpoint_matrix.json");
    assert_eq!(matrix.as_object().unwrap().len(), 26);
    assert_eq!(Endpoint::ALL.len(), 28);
    for (model, endpoints) in matrix.as_object().unwrap() {
        for endpoint in Endpoint::ALL {
            let actual = endpoint.definition_for_model(Some(model));
            let expected = &endpoints[endpoint.as_str()];
            match actual {
                None => assert!(expected.is_null(), "{model}: {endpoint:?}"),
                Some(actual) => {
                    assert_eq!(
                        actual.path,
                        expected["path"].as_str().unwrap(),
                        "{model}: {endpoint:?}"
                    );
                    let mode = match actual.mode {
                        renault_api::endpoints::EndpointMode::Default => "default",
                        renault_api::endpoints::EndpointMode::KcaStop => "kca-stop",
                        renault_api::endpoints::EndpointMode::Kcm => "kcm",
                        renault_api::endpoints::EndpointMode::KcmPauseResume => "kcm-pause-resume",
                        renault_api::endpoints::EndpointMode::KcmSettings => "kcm-settings",
                    };
                    assert_eq!(mode, expected["mode"].as_str().unwrap());
                }
            }
        }
    }
}

#[tokio::test]
async fn every_remaining_vehicle_data_fixture_decodes_through_public_methods() {
    let server = MockServer::start().await;
    mock_details(&server, "UNKNOWN").await;
    let api = authenticated(&server).await;
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/reference/vehicle_data");
    let (start, end) = range();
    let mut count = 0;
    for entry in std::fs::read_dir(directory).unwrap() {
        let file = entry.unwrap().path();
        let name = file.file_name().unwrap().to_str().unwrap();
        if ["battery-status", "cockpit", "location"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
        {
            continue;
        }
        let endpoint = if name == "no_data.json" {
            "charge-mode"
        } else {
            name.split('.').next().unwrap()
        };
        let endpoint = if endpoint.starts_with("charges-") {
            "charges"
        } else {
            endpoint
        };
        let value: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        let scoped = Mock::given(method("GET"))
            .and(path(data_path(endpoint, 1)))
            .respond_with(ResponseTemplate::new(200).set_body_json(value))
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        let result = match endpoint {
            "charge-mode" => api.charge_mode(ACCOUNT, VIN).await.map(|_| ()),
            "charge-schedule" => api.charge_schedule(ACCOUNT, VIN).await.map(|_| ()),
            "charging-settings" => api.charging_settings(ACCOUNT, VIN).await.map(|_| ()),
            "hvac-status" => api.hvac_status(ACCOUNT, VIN).await.map(|_| ()),
            "hvac-settings" => api.hvac_settings(ACCOUNT, VIN).await.map(|_| ()),
            "hvac-history" => api
                .hvac_history(ACCOUNT, VIN, start, end, HistoryPeriod::Day)
                .await
                .map(|_| ()),
            "hvac-sessions" => api
                .hvac_sessions(ACCOUNT, VIN, start, end)
                .await
                .map(|_| ()),
            "notification-settings" => api.notification_settings(ACCOUNT, VIN).await.map(|_| ()),
            "lock-status" => api
                .lock_status(ACCOUNT, VIN)
                .await
                .map(|s| assert_eq!(s.driver.as_deref(), Some("closed"))),
            "pressure" => api.tyre_pressure(ACCOUNT, VIN).await.map(|_| ()),
            "res-state" => api.res_state(ACCOUNT, VIN).await.map(|_| ()),
            "charge-history" => api
                .charge_history(ACCOUNT, VIN, start, end, HistoryPeriod::Day)
                .await
                .map(|_| ()),
            "charges" => api.charges(ACCOUNT, VIN, start, end).await.map(|_| ()),
            other => panic!("missing public read for fixture {other}: {name}"),
        };
        result.unwrap_or_else(|e| panic!("{name}: {e}"));
        drop(scoped);
        count += 1;
    }
    assert!(count >= 25);
}

#[tokio::test]
async fn car_adapters_contracts_and_alerts_preserve_the_reference_data() {
    let server = MockServer::start().await;
    mock_details(&server, "UNKNOWN").await;
    let api = authenticated(&server).await;
    for name in ["duster", "spring", "twingo_ze", "zoe_40", "zoe_50"] {
        let scoped = Mock::given(method("GET"))
            .and(path(format!(
                "/commerce/v1/accounts/{ACCOUNT}/kamereon/kca/car-adapter/v2/cars/{VIN}"
            )))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(reference(&format!("vehicle_gateway/{name}.1.json"))),
            )
            .mount_as_scoped(&server)
            .await;
        let adapter = api.car_adapter(ACCOUNT, VIN).await.unwrap();
        assert!(adapter.model_code.is_some());
        drop(scoped);
    }
    for name in ["fr_FR.1.json", "fr_FR.2.json"] {
        let value = reference(&format!("vehicle_contract/{name}"));
        let expected = value.as_array().unwrap().len();
        let scoped = Mock::given(method("GET"))
            .and(path(format!(
                "/commerce/v1/accounts/{ACCOUNT}/vehicles/{VIN}/contracts"
            )))
            .and(query_param("locale", "fr_FR"))
            .and(query_param("country", "FR"))
            .and(query_param("brand", "RENAULT"))
            .and(query_param("connectedServicesContracts", "true"))
            .and(query_param("warranty", "true"))
            .and(query_param("warrantyMaintenanceContracts", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(value))
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        let contracts = api.contracts(ACCOUNT, VIN).await.unwrap();
        assert_eq!(contracts.len(), expected);
        drop(scoped);
    }
    mock_json(
        &server,
        "GET",
        &route("UNKNOWN", Endpoint::Alerts),
        reference("vehicle_info/alerts.json"),
    )
    .await;
    let alerts = api.alerts(ACCOUNT, VIN).await.unwrap();
    assert!(!alerts.is_empty());
    assert!(alerts[0].short_description.is_some());
    assert!(alerts[0].extra.contains_key("criticityName"));
}

#[tokio::test]
async fn all_kcm_soc_and_settings_fixtures_are_typed() {
    let server = MockServer::start().await;
    mock_details(&server, "R5E1VE").await;
    let api = authenticated(&server).await;
    mock_json(
        &server,
        "GET",
        &route("R5E1VE", Endpoint::SocLevels),
        reference("vehicle_kcm_data/ev-soc-levels.json"),
    )
    .await;
    let soc = api.battery_soc(ACCOUNT, VIN).await.unwrap();
    assert_eq!(soc.soc_min, Some(20));
    assert_eq!(soc.soc_target, Some(80));
    assert!(soc.updated_at.is_some());
    for name in [
        "empty",
        "multi.active",
        "single.active",
        "single.inactive",
        "single",
    ] {
        let scoped = Mock::given(method("GET"))
            .and(path(route("R5E1VE", Endpoint::ChargeSchedule)))
            .respond_with(ResponseTemplate::new(200).set_body_json(reference(&format!(
                "vehicle_kcm_data/ev-settings.{name}.json"
            ))))
            .mount_as_scoped(&server)
            .await;
        assert!(matches!(
            api.charge_schedule(ACCOUNT, VIN).await.unwrap(),
            ChargeScheduleData::EvSettings(_)
        ));
        drop(scoped);
    }
}

#[tokio::test]
async fn every_supported_model_command_uses_its_exact_route_and_body() {
    let matrix = fixture("endpoint_matrix.json");
    let mut count = 0;
    for (model, definitions) in matrix.as_object().unwrap() {
        for endpoint in [
            Endpoint::SetChargeStart,
            Endpoint::SetChargeStop,
            Endpoint::SetHvacStop,
        ] {
            let definition = &definitions[endpoint.as_str()];
            if definition.is_null() {
                continue;
            }
            let mode = definition["mode"].as_str().unwrap();
            let server = MockServer::start().await;
            mock_details(&server, model).await;
            let api = authenticated(&server).await;
            let endpoint_path = route(model, endpoint);
            let expected = match (endpoint, mode) {
                (Endpoint::SetChargeStart, "kcm-settings") => {
                    let mut original = reference("vehicle_kcm_data/ev-settings.multi.active.json");
                    original["unknownFutureSetting"] = json!({"nested":[1,2,3]});
                    mock_json(&server, "GET", &endpoint_path, original.clone()).await;
                    for program in original["programs"].as_array_mut().unwrap() {
                        program["programActivationStatus"] = json!(false);
                    }
                    original
                }
                (Endpoint::SetChargeStart, "kcm-pause-resume") => {
                    json!({"data":{"type":"ChargePauseResume","attributes":{"action":"resume"}}})
                }
                (Endpoint::SetChargeStart, _) => {
                    json!({"data":{"type":"ChargingStart","attributes":{"action":"start"}}})
                }
                (Endpoint::SetChargeStop, "kcm-pause-resume") => {
                    json!({"data":{"type":"ChargePauseResume","attributes":{"action":"pause"}}})
                }
                (Endpoint::SetChargeStop, _) => {
                    json!({"data":{"type":"ChargingStart","attributes":{"action":"stop"}}})
                }
                (Endpoint::SetHvacStop, "kca-stop") => {
                    json!({"data":{"type":"HvacStart","attributes":{"action":"stop"}}})
                }
                (Endpoint::SetHvacStop, _) => {
                    json!({"data":{"type":"HvacStart","attributes":{"action":"cancel"}}})
                }
                _ => unreachable!(),
            };
            Mock::given(method("POST"))
                .and(path(&endpoint_path))
                .and(body_json(&expected))
                .respond_with(ResponseTemplate::new(200).set_body_json(expected.clone()))
                .expect(1)
                .mount(&server)
                .await;
            let result = match endpoint {
                Endpoint::SetChargeStart => api.start_charge(ACCOUNT, VIN, None).await,
                Endpoint::SetChargeStop => api.stop_charge(ACCOUNT, VIN).await,
                Endpoint::SetHvacStop => api.stop_hvac(ACCOUNT, VIN).await,
                _ => unreachable!(),
            };
            result.unwrap_or_else(|e| panic!("{model} {endpoint:?}: {e}"));
            count += 1;
        }
    }
    assert!(count > 40);
}

#[tokio::test]
async fn remaining_commands_send_golden_payloads_and_decode_receipts() {
    let server = MockServer::start().await;
    mock_details(&server, "UNKNOWN").await;
    let api = authenticated(&server).await;
    for (endpoint, body, fixture_name) in [
        (
            Endpoint::SetHornStart,
            json!({"data":{"type":"HornLights","attributes":{"action":"start","target":"horn"}}}),
            "horn-lights.horn.json",
        ),
        (
            Endpoint::SetLightsStart,
            json!({"data":{"type":"HornLights","attributes":{"action":"start","target":"lights"}}}),
            "horn-lights.lights.json",
        ),
        (
            Endpoint::SetRefreshLocation,
            json!({"data":{"type":"RefreshLocation"}}),
            "refresh-location.json",
        ),
        (
            Endpoint::SetChargeSetMode,
            json!({"data":{"type":"ChargeMode","attributes":{"action":"schedule_mode"}}}),
            "charge-mode.schedule_mode.json",
        ),
        (
            Endpoint::SetHvacStart,
            json!({"data":{"type":"HvacStart","attributes":{"action":"start","targetTemperature":21.0}}}),
            "hvac-start.start.json",
        ),
    ] {
        let scoped = Mock::given(method("POST"))
            .and(path(route("UNKNOWN", endpoint)))
            .and(body_json(body))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(reference(&format!("vehicle_action/{fixture_name}"))),
            )
            .expect(1)
            .mount_as_scoped(&server)
            .await;
        let receipt = match endpoint {
            Endpoint::SetHornStart => api.start_horn(ACCOUNT, VIN).await,
            Endpoint::SetLightsStart => api.start_lights(ACCOUNT, VIN).await,
            Endpoint::SetRefreshLocation => api.refresh_location(ACCOUNT, VIN).await,
            Endpoint::SetChargeSetMode => {
                api.set_charge_mode(ACCOUNT, VIN, ChargeModeRequest::Scheduled)
                    .await
            }
            Endpoint::SetHvacStart => api.start_hvac(ACCOUNT, VIN, 21.0, None).await,
            _ => unreachable!(),
        }
        .unwrap();
        assert_eq!(receipt.id.as_deref(), Some("guid"));
        drop(scoped);
    }
    Mock::given(method("POST"))
        .and(path(route("UNKNOWN", Endpoint::SocLevels)))
        .and(body_json(json!({"socMin":20,"socTarget":80})))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(reference("vehicle_kcm_action/ev-soc-levels.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        api.set_battery_soc(ACCOUNT, VIN, 20, 80)
            .await
            .unwrap()
            .command_id
            .as_deref(),
        Some("commandId-1234")
    );
}

#[tokio::test]
async fn schedule_payloads_have_utc_times_and_seven_explicit_days() {
    let server = MockServer::start().await;
    mock_details(&server, "UNKNOWN").await;
    let api = authenticated(&server).await;
    let expected = json!({"data":{"type":"ChargeSchedule","attributes":{"schedules":[{"id":1,"activated":true,"monday":{"startTime":"T23:30Z","duration":90},"tuesday":null,"wednesday":null,"thursday":null,"friday":null,"saturday":null,"sunday":null}]}}});
    Mock::given(method("POST"))
        .and(path(route("UNKNOWN", Endpoint::SetChargeSetSchedule)))
        .and(body_json(expected.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(expected))
        .expect(1)
        .mount(&server)
        .await;
    let schedule = ChargeSchedule {
        id: Some(1),
        activated: Some(true),
        monday: Some(ChargeDay {
            start_time: Some(ScheduleTime::new(23, 30).unwrap()),
            duration_minutes: Some(90),
        }),
        ..Default::default()
    };
    let result = api
        .set_charge_schedules(ACCOUNT, VIN, &[schedule])
        .await
        .unwrap();
    assert_eq!(result.attributes.charge_schedules.unwrap().len(), 1);
    let expected = json!({"data":{"type":"HvacSchedule","attributes":{"schedules":[{"id":2,"activated":true,"monday":null,"tuesday":null,"wednesday":null,"thursday":null,"friday":null,"saturday":null,"sunday":{"readyAtTime":"T07:15Z"}}]}}});
    Mock::given(method("POST"))
        .and(path(route("UNKNOWN", Endpoint::SetHvacSetSchedule)))
        .and(body_json(expected.clone()))
        .respond_with(ResponseTemplate::new(200).set_body_json(expected))
        .expect(1)
        .mount(&server)
        .await;
    let schedule = HvacSchedule {
        id: Some(2),
        activated: Some(true),
        sunday: Some(HvacDay {
            ready_at_time: Some(ScheduleTime::new(7, 15).unwrap()),
        }),
        ..Default::default()
    };
    assert_eq!(
        api.set_hvac_schedules(ACCOUNT, VIN, &[schedule])
            .await
            .unwrap()
            .attributes
            .hvac_schedules
            .unwrap()
            .len(),
        1
    );
}

#[tokio::test]
async fn histories_and_delayed_commands_use_explicit_dates() {
    let server = MockServer::start().await;
    mock_details(&server, "UNKNOWN").await;
    let api = authenticated(&server).await;
    let (start, end) = range();
    Mock::given(method("GET"))
        .and(path(route("UNKNOWN", Endpoint::ChargeHistory)))
        .and(query_param("start", "202011"))
        .and(query_param("end", "202012"))
        .and(query_param("type", "month"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(reference("vehicle_data/charge-history.month.json")),
        )
        .expect(1)
        .mount(&server)
        .await;
    assert_eq!(
        api.charge_history(ACCOUNT, VIN, start, end, HistoryPeriod::Month)
            .await
            .unwrap()
            .duration_unit,
        DurationUnit::Seconds
    );
    mock_json(
        &server,
        "GET",
        &route("UNKNOWN", Endpoint::Charges),
        reference("vehicle_data/charges.json"),
    )
    .await;
    assert_eq!(
        api.charges(ACCOUNT, VIN, start, end)
            .await
            .unwrap()
            .charges
            .unwrap()[0]
            .charge_duration,
        Some(479.0)
    );
    let when = Utc.with_ymd_and_hms(2026, 3, 6, 23, 45, 0).unwrap();
    assert!(matches!(
        api.start_charge(ACCOUNT, VIN, Some(when)).await,
        Err(RenaultError::InvalidInput(_))
    ));
    let server = MockServer::start().await;
    mock_details(&server, "XCB1VE").await;
    let api = authenticated(&server).await;
    Mock::given(method("POST")).and(path(route("XCB1VE",Endpoint::SetChargeStart)))
        .and(body_json(json!({"data":{"type":"ChargingStart","attributes":{"action":"start","startDateTime":"2026-03-06T23:45:00Z"}}})))
        .respond_with(ResponseTemplate::new(200).set_body_json(reference("vehicle_kcm_action/charging-start.delayed.json"))).expect(1).mount(&server).await;
    assert!(
        api.start_charge(ACCOUNT, VIN, Some(when))
            .await
            .unwrap()
            .attributes
            .start_at
            .is_some()
    );
}

#[tokio::test]
async fn validation_and_unsupported_commands_have_no_side_effects() {
    let server = MockServer::start().await;
    let api = authenticated(&server).await;
    let (start, end) = range();
    for (min, target) in [(14, 80), (20, 54), (46, 80), (20, 101), (21, 80), (20, 81)] {
        assert!(matches!(
            api.set_battery_soc(ACCOUNT, VIN, min, target).await,
            Err(RenaultError::InvalidInput(_))
        ));
    }
    assert!(matches!(
        api.start_hvac(ACCOUNT, VIN, f64::NAN, None).await,
        Err(RenaultError::InvalidInput(_))
    ));
    assert!(matches!(
        api.charge_history(ACCOUNT, VIN, end, start, HistoryPeriod::Day)
            .await,
        Err(RenaultError::InvalidInput(_))
    ));
    let schedule = ChargeSchedule {
        id: Some(1),
        monday: Some(ChargeDay::default()),
        ..Default::default()
    };
    assert!(matches!(
        api.set_charge_schedules(ACCOUNT, VIN, &[schedule]).await,
        Err(RenaultError::InvalidInput(_))
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
    mock_details(&server, "R5E1VE").await;
    assert!(
        !api.supports_endpoint(ACCOUNT, VIN, Endpoint::SetChargeStop)
            .await
            .unwrap()
    );
    assert!(matches!(
        api.stop_charge(ACCOUNT, VIN).await,
        Err(RenaultError::UnsupportedEndpoint { .. })
    ));
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .all(|r| r.method == "GET")
    );
}

#[tokio::test]
async fn low_level_calls_remain_on_the_configured_host_and_do_not_retry_posts() {
    let server = MockServer::start().await;
    let api = authenticated(&server).await;
    for invalid in [
        "https://attacker.invalid/",
        "//attacker.invalid/",
        "/\\attacker.invalid/",
        "/test#fragment",
        "/test?country=GB",
    ] {
        assert!(matches!(
            api.http_get::<Value>(invalid).await,
            Err(RenaultError::InvalidInput(_))
        ));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    Mock::given(method("POST"))
        .and(path("/test"))
        .and(body_json(json!({"some":"value"})))
        .respond_with(ResponseTemplate::new(503).set_body_string("unavailable"))
        .expect(1)
        .mount(&server)
        .await;
    assert!(matches!(
        api.http_post::<Value>("/test", &json!({"some":"value"}))
            .await,
        Err(RenaultError::Http { status: 503, .. })
    ));
    mock_details(&server, "UNKNOWN").await;
    Mock::given(method("POST"))
        .and(path(route("UNKNOWN", Endpoint::SetRefreshLocation)))
        .respond_with(ResponseTemplate::new(204))
        .expect(1)
        .mount(&server)
        .await;
    let receipt = api.refresh_location(ACCOUNT, VIN).await.unwrap();
    assert!(receipt.id.is_none());
}

#[test]
fn schedule_helpers_validate_and_wrap_at_midnight() {
    assert_eq!(
        ScheduleTime::new(23, 30).unwrap().end_time(90).as_str(),
        "T01:00Z"
    );
    for invalid in ["T24:00Z", "T23:60Z", "12:00", "invalid", "Té:00Z"] {
        assert!(ScheduleTime::try_from(invalid.to_owned()).is_err());
    }
}
