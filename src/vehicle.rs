use chrono::{DateTime, NaiveDate, SecondsFormat, Utc};
use serde::de::DeserializeOwned;
use serde_json::json;

use super::*;
use crate::endpoints::{Endpoint, EndpointDefinition, EndpointMode};

impl RenaultApi {
    /// Resolve routing using the reference table. Unknown models use defaults.
    pub async fn endpoint_definition(
        &self,
        account_id: &str,
        vin: &str,
        endpoint: Endpoint,
    ) -> Result<EndpointDefinition> {
        validate_vehicle(account_id, vin)?;
        let model = self.model_code(account_id, vin).await?;
        self.resolve_endpoint(model.as_deref(), endpoint)
    }

    pub async fn supports_endpoint(
        &self,
        account_id: &str,
        vin: &str,
        endpoint: Endpoint,
    ) -> Result<bool> {
        validate_vehicle(account_id, vin)?;
        let model = self.model_code(account_id, vin).await?;
        Ok(endpoint.definition_for_model(model.as_deref()).is_some())
    }

    pub async fn car_adapter(&self, account_id: &str, vin: &str) -> Result<CarAdapter> {
        validate_vehicle(account_id, vin)?;
        wire::attributes(
            self.get(&[
                "commerce",
                "v1",
                "accounts",
                account_id,
                "kamereon",
                "kca",
                "car-adapter",
                "v2",
                "cars",
                vin,
            ])
            .await?,
        )
    }

    pub async fn contracts(&self, account_id: &str, vin: &str) -> Result<Vec<Contract>> {
        validate_vehicle(account_id, vin)?;
        let params = [
            ("locale", self.inner.config.locale.clone()),
            ("brand", "RENAULT".into()),
            ("connectedServicesContracts", "true".into()),
            ("warranty", "true".into()),
            ("warrantyMaintenanceContracts", "true".into()),
        ];
        let value = self
            .request(
                Method::GET,
                &[
                    "commerce",
                    "v1",
                    "accounts",
                    account_id,
                    "vehicles",
                    vin,
                    "contracts",
                ],
                &params,
                None,
            )
            .await?;
        let data = if value.is_array() {
            value
        } else {
            value
                .get("contractList")
                .cloned()
                .ok_or_else(|| invalid_response("missing contractList"))?
        };
        wire::decode(data, Service::Kamereon)
    }

    pub async fn alerts(&self, account_id: &str, vin: &str) -> Result<Vec<Alert>> {
        let value = self
            .read_endpoint(account_id, vin, Endpoint::Alerts, &[])
            .await?;
        let data = if value.is_array() {
            value
        } else {
            value
                .get("data")
                .cloned()
                .ok_or_else(|| invalid_response("missing alerts array"))?
        };
        wire::decode(data, Service::Kamereon)
    }

    pub async fn battery_soc(&self, account_id: &str, vin: &str) -> Result<BatterySoc> {
        wire::decode(
            self.read_endpoint(account_id, vin, Endpoint::SocLevels, &[])
                .await?,
            Service::Kamereon,
        )
    }
    pub async fn tyre_pressure(&self, account_id: &str, vin: &str) -> Result<TyrePressure> {
        self.read_attributes(account_id, vin, Endpoint::Pressure, &[])
            .await
    }
    pub async fn hvac_status(&self, account_id: &str, vin: &str) -> Result<HvacStatus> {
        self.read_attributes(account_id, vin, Endpoint::HvacStatus, &[])
            .await
    }
    pub async fn hvac_settings(&self, account_id: &str, vin: &str) -> Result<HvacSettings> {
        self.read_attributes(account_id, vin, Endpoint::HvacSettings, &[])
            .await
    }
    pub async fn charge_mode(&self, account_id: &str, vin: &str) -> Result<ChargeMode> {
        self.read_attributes(account_id, vin, Endpoint::ChargeMode, &[])
            .await
    }
    pub async fn charging_settings(&self, account_id: &str, vin: &str) -> Result<ChargingSettings> {
        self.read_attributes(account_id, vin, Endpoint::ChargingSettings, &[])
            .await
    }
    pub async fn notification_settings(
        &self,
        account_id: &str,
        vin: &str,
    ) -> Result<NotificationSettings> {
        self.read_attributes(account_id, vin, Endpoint::NotificationSettings, &[])
            .await
    }
    pub async fn res_state(&self, account_id: &str, vin: &str) -> Result<ResState> {
        self.read_attributes(account_id, vin, Endpoint::ResState, &[])
            .await
    }
    pub async fn lock_status(&self, account_id: &str, vin: &str) -> Result<LockStatus> {
        let mut value: Value = self
            .read_attributes(account_id, vin, Endpoint::LockStatus, &[])
            .await?;
        let object = value
            .as_object_mut()
            .ok_or_else(|| invalid_response("missing lock attributes"))?;
        for (output, primary, alias) in [
            ("rear_left", "doorStatusRearLeft", "statusDoorRearLeft"),
            ("rear_right", "doorStatusRearRight", "statusDoorRearRight"),
            ("driver", "doorStatusDriver", "statusDoorDriver"),
            ("passenger", "doorStatusPassenger", "statusDoorPassenger"),
            ("hatch", "hatchStatus", "statusDoorTailGate"),
        ] {
            let value = object
                .get(primary)
                .filter(|v| !v.is_null())
                .or_else(|| object.get(alias))
                .cloned()
                .unwrap_or(Value::Null);
            object.insert(output.into(), value);
        }
        wire::decode(value, Service::Kamereon)
    }

    pub async fn charge_schedule(&self, account_id: &str, vin: &str) -> Result<ChargeScheduleData> {
        let definition = self
            .endpoint_definition(account_id, vin, Endpoint::ChargeSchedule)
            .await?;
        let value = self
            .endpoint_json(account_id, vin, definition, Method::GET, &[], None)
            .await?;
        if definition.mode == EndpointMode::KcmSettings {
            Ok(ChargeScheduleData::EvSettings(wire::decode(
                value,
                Service::Kamereon,
            )?))
        } else {
            Ok(ChargeScheduleData::Calendar(wire::attributes(value)?))
        }
    }

    pub async fn charge_history(
        &self,
        account_id: &str,
        vin: &str,
        start: NaiveDate,
        end: NaiveDate,
        period: HistoryPeriod,
    ) -> Result<ChargeHistory> {
        let params = history_params(start, end, Some(period))?;
        let value: Value = self
            .read_attributes(account_id, vin, Endpoint::ChargeHistory, &params)
            .await?;
        let summaries = wire::decode(
            value.get("chargeSummaries").cloned().unwrap_or(Value::Null),
            Service::Kamereon,
        )?;
        let model = self.model_code(account_id, vin).await?;
        Ok(ChargeHistory {
            summaries,
            duration_unit: DurationUnit::for_model(model.as_deref()),
        })
    }
    pub async fn charges(
        &self,
        account_id: &str,
        vin: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<Charges> {
        let params = history_params(start, end, None)?;
        let value: Value = self
            .read_attributes(account_id, vin, Endpoint::Charges, &params)
            .await?;
        let charges = wire::decode(
            value.get("charges").cloned().unwrap_or(Value::Null),
            Service::Kamereon,
        )?;
        let model = self.model_code(account_id, vin).await?;
        Ok(Charges {
            charges,
            duration_unit: DurationUnit::for_model(model.as_deref()),
            instantaneous_power_in_watts: model.as_deref() == Some("X101VE"),
        })
    }
    pub async fn hvac_history(
        &self,
        account_id: &str,
        vin: &str,
        start: NaiveDate,
        end: NaiveDate,
        period: HistoryPeriod,
    ) -> Result<HvacHistory> {
        self.read_attributes(
            account_id,
            vin,
            Endpoint::HvacHistory,
            &history_params(start, end, Some(period))?,
        )
        .await
    }
    pub async fn hvac_sessions(
        &self,
        account_id: &str,
        vin: &str,
        start: NaiveDate,
        end: NaiveDate,
    ) -> Result<HvacSessions> {
        self.read_attributes(
            account_id,
            vin,
            Endpoint::HvacSessions,
            &history_params(start, end, None)?,
        )
        .await
    }

    pub async fn set_battery_soc(
        &self,
        account_id: &str,
        vin: &str,
        min: u8,
        target: u8,
    ) -> Result<CommandReceipt> {
        if !(15..=45).contains(&min)
            || !min.is_multiple_of(5)
            || !(55..=100).contains(&target)
            || !target.is_multiple_of(5)
        {
            return Err(RenaultError::InvalidInput(
                "SoC minimum must be 15..45 and target 55..100, in steps of 5".into(),
            ));
        }
        self.command(
            account_id,
            vin,
            Endpoint::SocLevels,
            json!({"socMin":min,"socTarget":target}),
        )
        .await
    }
    pub async fn start_hvac(
        &self,
        account_id: &str,
        vin: &str,
        temperature_celsius: f64,
        when: Option<DateTime<Utc>>,
    ) -> Result<CommandReceipt> {
        if !temperature_celsius.is_finite() {
            return Err(RenaultError::InvalidInput(
                "HVAC temperature must be finite".into(),
            ));
        }
        let mut attributes = json!({"action":"start","targetTemperature":temperature_celsius});
        if let Some(when) = when {
            attributes["startDateTime"] = json!(when.to_rfc3339_opts(SecondsFormat::Secs, true));
        }
        self.command(
            account_id,
            vin,
            Endpoint::SetHvacStart,
            action_body("HvacStart", attributes),
        )
        .await
    }
    pub async fn stop_hvac(&self, account_id: &str, vin: &str) -> Result<CommandReceipt> {
        let definition = self
            .endpoint_definition(account_id, vin, Endpoint::SetHvacStop)
            .await?;
        let action = if definition.mode == EndpointMode::KcaStop {
            "stop"
        } else {
            "cancel"
        };
        self.send_command(
            account_id,
            vin,
            definition,
            action_body("HvacStart", json!({"action":action})),
        )
        .await
    }
    pub async fn set_hvac_schedules(
        &self,
        account_id: &str,
        vin: &str,
        schedules: &[HvacSchedule],
    ) -> Result<CommandReceipt> {
        validate_schedule_ids(schedules)?;
        for schedule in schedules {
            for day in schedule.days().into_iter().flatten() {
                if day.ready_at_time.is_none() {
                    return Err(RenaultError::InvalidInput(
                        "HVAC day requires ready_at_time".into(),
                    ));
                }
            }
        }
        self.command(
            account_id,
            vin,
            Endpoint::SetHvacSetSchedule,
            action_body("HvacSchedule", json!({"schedules":schedules})),
        )
        .await
    }
    pub async fn set_charge_schedules(
        &self,
        account_id: &str,
        vin: &str,
        schedules: &[ChargeSchedule],
    ) -> Result<CommandReceipt> {
        validate_schedule_ids(schedules)?;
        for schedule in schedules {
            for day in schedule.days().into_iter().flatten() {
                if day.start_time.is_none()
                    || !day
                        .duration_minutes
                        .is_some_and(|d| (1..=1440).contains(&d))
                {
                    return Err(RenaultError::InvalidInput(
                        "charge day requires start_time and duration in 1..1440 minutes".into(),
                    ));
                }
            }
        }
        self.command(
            account_id,
            vin,
            Endpoint::SetChargeSetSchedule,
            action_body("ChargeSchedule", json!({"schedules":schedules})),
        )
        .await
    }
    pub async fn set_charge_mode(
        &self,
        account_id: &str,
        vin: &str,
        mode: ChargeModeRequest,
    ) -> Result<CommandReceipt> {
        self.command(
            account_id,
            vin,
            Endpoint::SetChargeSetMode,
            action_body("ChargeMode", json!({"action":mode.as_str()})),
        )
        .await
    }

    /// KCM settings vehicles start charging by disabling ALL scheduled programs,
    /// including preconditioning. No automatic retry or restoration is performed.
    pub async fn start_charge(
        &self,
        account_id: &str,
        vin: &str,
        when: Option<DateTime<Utc>>,
    ) -> Result<CommandReceipt> {
        let definition = self
            .endpoint_definition(account_id, vin, Endpoint::SetChargeStart)
            .await?;
        if when.is_some() && definition.mode != EndpointMode::Kcm {
            return Err(RenaultError::InvalidInput(
                "delayed charging is not supported by this endpoint".into(),
            ));
        }
        let body = match definition.mode {
            EndpointMode::KcmSettings => {
                let mut current = self
                    .endpoint_json(account_id, vin, definition, Method::GET, &[], None)
                    .await?;
                let programs = current
                    .get_mut("programs")
                    .and_then(Value::as_array_mut)
                    .ok_or_else(|| invalid_response("ev/settings requires a programs array"))?;
                for program in programs {
                    let object = program
                        .as_object_mut()
                        .ok_or_else(|| invalid_response("invalid EV program"))?;
                    object.insert("programActivationStatus".into(), json!(false));
                }
                // Preserve every unknown field and its exact JSON representation.
                current
            }
            EndpointMode::KcmPauseResume => {
                action_body("ChargePauseResume", json!({"action":"resume"}))
            }
            _ => {
                let mut attributes = json!({"action":"start"});
                if let Some(when) = when {
                    attributes["startDateTime"] =
                        json!(when.to_rfc3339_opts(SecondsFormat::Secs, true));
                }
                action_body("ChargingStart", attributes)
            }
        };
        self.send_command(account_id, vin, definition, body).await
    }
    pub async fn stop_charge(&self, account_id: &str, vin: &str) -> Result<CommandReceipt> {
        let definition = self
            .endpoint_definition(account_id, vin, Endpoint::SetChargeStop)
            .await?;
        let (kind, action) = if definition.mode == EndpointMode::KcmPauseResume {
            ("ChargePauseResume", "pause")
        } else {
            ("ChargingStart", "stop")
        };
        self.send_command(
            account_id,
            vin,
            definition,
            action_body(kind, json!({"action":action})),
        )
        .await
    }
    pub async fn start_horn(&self, account_id: &str, vin: &str) -> Result<CommandReceipt> {
        self.command(
            account_id,
            vin,
            Endpoint::SetHornStart,
            action_body("HornLights", json!({"action":"start","target":"horn"})),
        )
        .await
    }
    pub async fn start_lights(&self, account_id: &str, vin: &str) -> Result<CommandReceipt> {
        self.command(
            account_id,
            vin,
            Endpoint::SetLightsStart,
            action_body("HornLights", json!({"action":"start","target":"lights"})),
        )
        .await
    }
    pub async fn refresh_location(&self, account_id: &str, vin: &str) -> Result<CommandReceipt> {
        self.command(
            account_id,
            vin,
            Endpoint::SetRefreshLocation,
            json!({"data":{"type":"RefreshLocation"}}),
        )
        .await
    }

    /// Low-level authenticated access, restricted to the configured Kamereon host.
    pub async fn http_get<T: DeserializeOwned>(&self, endpoint: &str) -> Result<T> {
        let value = self
            .request_url(Method::GET, self.relative_url(endpoint)?, &[], None)
            .await?;
        wire::decode(value, Service::Kamereon)
    }
    /// Explicit low-level POST. The body is sent unchanged and never retried.
    pub async fn http_post<T: DeserializeOwned>(&self, endpoint: &str, body: &Value) -> Result<T> {
        let value = self
            .request_url(Method::POST, self.relative_url(endpoint)?, &[], Some(body))
            .await?;
        wire::decode(value, Service::Kamereon)
    }

    fn relative_url(&self, endpoint: &str) -> Result<Url> {
        if !endpoint.starts_with('/')
            || endpoint.starts_with("//")
            || endpoint.contains(['\\', '#'])
        {
            return Err(RenaultError::InvalidInput(
                "endpoint must be a relative absolute-path without a fragment".into(),
            ));
        }
        let root = &self.inner.config.endpoints.kamereon_url;
        let url = root
            .join(endpoint.trim_start_matches('/'))
            .map_err(|_| RenaultError::InvalidInput("invalid endpoint path".into()))?;
        if url.origin() != root.origin() || url.query_pairs().any(|(key, _)| key == "country") {
            return Err(RenaultError::InvalidInput(
                "endpoint must use the configured host and country".into(),
            ));
        }
        Ok(url)
    }

    pub(super) fn resolve_endpoint(
        &self,
        model: Option<&str>,
        endpoint: Endpoint,
    ) -> Result<EndpointDefinition> {
        endpoint
            .definition_for_model(model)
            .ok_or_else(|| RenaultError::UnsupportedEndpoint {
                endpoint: endpoint.as_str(),
                model_code: model.unwrap_or("unknown").into(),
            })
    }
    async fn read_endpoint(
        &self,
        account_id: &str,
        vin: &str,
        endpoint: Endpoint,
        params: &[(&str, String)],
    ) -> Result<Value> {
        let definition = self.endpoint_definition(account_id, vin, endpoint).await?;
        self.endpoint_json(account_id, vin, definition, Method::GET, params, None)
            .await
    }
    async fn read_attributes<T: DeserializeOwned>(
        &self,
        account_id: &str,
        vin: &str,
        endpoint: Endpoint,
        params: &[(&str, String)],
    ) -> Result<T> {
        wire::attributes(
            self.read_endpoint(account_id, vin, endpoint, params)
                .await?,
        )
    }
    async fn command(
        &self,
        account_id: &str,
        vin: &str,
        endpoint: Endpoint,
        body: Value,
    ) -> Result<CommandReceipt> {
        let definition = self.endpoint_definition(account_id, vin, endpoint).await?;
        self.send_command(account_id, vin, definition, body).await
    }
    async fn send_command(
        &self,
        account_id: &str,
        vin: &str,
        definition: EndpointDefinition,
        body: Value,
    ) -> Result<CommandReceipt> {
        command_receipt(
            self.endpoint_json(account_id, vin, definition, Method::POST, &[], Some(&body))
                .await?,
        )
    }
    pub(super) async fn endpoint_json(
        &self,
        account_id: &str,
        vin: &str,
        definition: EndpointDefinition,
        method: Method,
        params: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<Value> {
        validate_vehicle(account_id, vin)?;
        let mut path = vec!["commerce", "v1", "accounts", account_id, "kamereon"];
        path.extend(
            definition
                .path
                .trim_start_matches('/')
                .split('/')
                .map(|part| if part == "{vin}" { vin } else { part }),
        );
        self.request(method, &path, params, body).await
    }
}

fn invalid_response(context: &'static str) -> RenaultError {
    RenaultError::InvalidResponse {
        service: Service::Kamereon,
        context,
    }
}
fn history_params(
    start: NaiveDate,
    end: NaiveDate,
    period: Option<HistoryPeriod>,
) -> Result<Vec<(&'static str, String)>> {
    if start > end {
        return Err(RenaultError::InvalidInput(
            "history start must be on or before end".into(),
        ));
    }
    let format = period.unwrap_or(HistoryPeriod::Day).date_format();
    let mut params = vec![
        ("start", start.format(format).to_string()),
        ("end", end.format(format).to_string()),
    ];
    if let Some(period) = period {
        params.push(("type", period.as_str().into()));
    }
    Ok(params)
}
fn action_body(kind: &str, attributes: Value) -> Value {
    json!({"data":{"type":kind,"attributes":attributes}})
}
fn validate_schedule_ids<D>(schedules: &[WeeklySchedule<D>]) -> Result<()> {
    let mut ids = std::collections::HashSet::new();
    for schedule in schedules {
        let id = schedule
            .id
            .filter(|id| *id > 0)
            .ok_or_else(|| RenaultError::InvalidInput("schedule requires a positive ID".into()))?;
        if !ids.insert(id) {
            return Err(RenaultError::InvalidInput("duplicate schedule ID".into()));
        }
    }
    Ok(())
}
fn command_receipt(value: Value) -> Result<CommandReceipt> {
    if !value.is_null() && !value.is_object() {
        return Err(invalid_response("invalid command acknowledgement"));
    }
    let string = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    let kind = string("/data/type").or_else(|| string("/type"));
    let mut attributes = value
        .pointer("/data/attributes")
        .cloned()
        .unwrap_or(json!({}));
    let schedules = attributes
        .as_object_mut()
        .and_then(|a| a.remove("schedules"));
    let mut attributes: CommandAttributes = wire::decode(attributes, Service::Kamereon)?;
    if let Some(schedules) = schedules {
        match kind.as_deref() {
            Some("ChargeSchedule") => {
                attributes.charge_schedules = Some(wire::decode(schedules, Service::Kamereon)?)
            }
            Some("HvacSchedule") => {
                attributes.hvac_schedules = Some(wire::decode(schedules, Service::Kamereon)?)
            }
            _ => {
                attributes.extra.insert("schedules".into(), schedules);
            }
        }
    }
    let ev_settings = if value.get("programs").is_some() || value.get("chargeModeRq").is_some() {
        Some(wire::decode(value.clone(), Service::Kamereon)?)
    } else {
        None
    };
    Ok(CommandReceipt {
        id: string("/data/id"),
        kind,
        command_id: string("/commandId"),
        status: string("/status"),
        attributes,
        ev_settings,
    })
}
