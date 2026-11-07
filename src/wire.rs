//! Upstream JSON shapes, intentionally separate from the public API.

use chrono::{DateTime, Utc};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::Value;

use crate::{ApiErrorKind, RenaultError, Result, Service, models::*};

pub(crate) fn decode<T: DeserializeOwned>(value: Value, service: Service) -> Result<T> {
    serde_json::from_value(value).map_err(|_| RenaultError::InvalidResponse {
        service,
        context: "unexpected JSON structure or field value",
    })
}

pub(crate) fn check_error(
    value: &Value,
    service: Service,
    status: u16,
    verifying_code: bool,
) -> Result<()> {
    match service {
        Service::Configuration => Ok(()),
        Service::Gigya => {
            let code = value.get("errorCode").and_then(Value::as_i64).ok_or(
                RenaultError::InvalidResponse {
                    service,
                    context: "missing Gigya errorCode",
                },
            )?;
            match code {
                0 => Ok(()),
                403042 => Err(RenaultError::InvalidCredentials),
                403005 | 403013 => Err(RenaultError::AuthenticationExpired),
                400006 if verifying_code => Err(RenaultError::InvalidVerificationCode),
                _ => Err(RenaultError::Api {
                    service,
                    code: code.to_string(),
                    kind: ApiErrorKind::Other,
                    status,
                }),
            }
        }
        Service::Kamereon => {
            let Some(error) = value
                .get("errors")
                .and_then(Value::as_array)
                .and_then(|e| e.first())
            else {
                return Ok(());
            };
            let code = error
                .get("errorCode")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            if code == "err.func.wired.unauthorized" {
                return Err(RenaultError::AuthenticationExpired);
            }
            let kind = match code {
                "err.func.400" => ApiErrorKind::InvalidInput,
                "err.func.403" | "err.func.wired.forbidden" => ApiErrorKind::AccessDenied,
                "err.tech.501" => ApiErrorKind::NotSupported,
                "err.func.wired.notFound" | "err.func.wired.not-found" => ApiErrorKind::NotFound,
                "err.func.wired.overloaded" => ApiErrorKind::QuotaExceeded,
                "err.func.privacy.on" => ApiErrorKind::PrivacyMode,
                "409001" => ApiErrorKind::ChargeModeInProgress,
                "err.func.wired.low.battery.level" => ApiErrorKind::InsufficientBattery,
                "err.tech.500" | "err.tech.wired.kamereon-proxy" => ApiErrorKind::Upstream,
                _ => ApiErrorKind::Other,
            };
            Err(RenaultError::Api {
                service,
                code: code.into(),
                kind,
                status,
            })
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct Person {
    pub accounts: Vec<WireAccount>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireAccount {
    account_id: Option<String>,
    account_type: Option<String>,
    account_status: Option<String>,
}

impl WireAccount {
    pub fn into_public(self) -> Option<Account> {
        Some(Account {
            id: self.account_id.filter(|id| !id.is_empty())?,
            account_type: self.account_type,
            status: self.account_status,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Vehicles {
    pub vehicle_links: Vec<WireVehicle>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireVehicle {
    vin: Option<String>,
    status: Option<String>,
    vehicle_details: Option<Details>,
}

impl WireVehicle {
    pub fn into_public(self) -> Option<Vehicle> {
        Some(Vehicle {
            vin: self.vin.filter(|vin| !vin.is_empty())?,
            status: self.status,
            details: self.vehicle_details.map(Into::into),
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Details {
    vin: Option<String>,
    registration_number: Option<String>,
    radio_code: Option<String>,
    brand: Option<CodeLabel>,
    model: Option<CodeLabel>,
    energy: Option<CodeLabel>,
    engine_energy_type: Option<String>,
    assets: Option<Vec<WireAsset>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireAsset {
    asset_type: Option<String>,
    asset_role: Option<String>,
    title: Option<String>,
    renditions: Option<Vec<WireRendition>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WireRendition {
    resolution_type: Option<String>,
    url: Option<String>,
}

impl From<Details> for VehicleDetails {
    fn from(v: Details) -> Self {
        Self {
            vin: v.vin,
            registration_number: v.registration_number,
            radio_code: v.radio_code,
            brand: v.brand,
            model: v.model,
            energy: v.energy,
            engine_energy_type: v.engine_energy_type,
            assets: v
                .assets
                .unwrap_or_default()
                .into_iter()
                .map(|a| Asset {
                    asset_type: a.asset_type,
                    role: a.asset_role,
                    title: a.title,
                    renditions: a
                        .renditions
                        .unwrap_or_default()
                        .into_iter()
                        .map(|r| Rendition {
                            resolution_type: r.resolution_type,
                            url: r.url,
                        })
                        .collect(),
                })
                .collect(),
        }
    }
}

pub(crate) fn attributes<T: DeserializeOwned>(value: Value) -> Result<T> {
    let data =
        value
            .get("data")
            .filter(|data| data.is_object())
            .ok_or(RenaultError::InvalidResponse {
                service: Service::Kamereon,
                context: "missing data object",
            })?;
    let attributes = match data.get("attributes") {
        None | Some(Value::Null) => serde_json::json!({}),
        Some(value) if value.is_object() => value.clone(),
        _ => {
            return Err(RenaultError::InvalidResponse {
                service: Service::Kamereon,
                context: "invalid attributes object",
            });
        }
    };
    decode(attributes, Service::Kamereon)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Battery {
    timestamp: Option<DateTime<Utc>>,
    battery_level: Option<u8>,
    battery_temperature: Option<f64>,
    battery_autonomy: Option<f64>,
    plug_status: Option<i32>,
    charging_status: Option<f64>,
    charging_instantaneous_power: Option<f64>,
    charging_remaining_time: Option<u32>,
    charging_remaining_time_last_update_date_time: Option<DateTime<Utc>>,
    battery_capacity: Option<f64>,
    battery_available_energy: Option<f64>,
    #[serde(rename = "V2L_SystemStatusDisplay")]
    v2l_system_status: Option<i32>,
}

impl Battery {
    pub fn into_public(self, model_code: Option<&str>) -> Result<BatteryStatus> {
        if self.battery_level.is_some_and(|level| level > 100) {
            return Err(RenaultError::InvalidResponse {
                service: Service::Kamereon,
                context: "batteryLevel is outside 0..=100",
            });
        }
        Ok(BatteryStatus {
            updated_at: self.timestamp,
            level_percent: self.battery_level,
            temperature_celsius: self.battery_temperature,
            range_km: self.battery_autonomy,
            plug_status: self.plug_status.map(Into::into),
            charging_status: self.charging_status.map(Into::into),
            charging_power_kw: self.charging_instantaneous_power.map(|p| {
                if model_code == Some("X101VE") {
                    p / 1000.0
                } else {
                    p
                }
            }),
            charging_remaining_minutes: self.charging_remaining_time,
            charging_remaining_updated_at: self.charging_remaining_time_last_update_date_time,
            capacity_raw: self.battery_capacity,
            available_energy_raw: self.battery_available_energy,
            v2l_system_status: self.v2l_system_status,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireCockpit {
    total_mileage: Option<f64>,
    fuel_autonomy: Option<f64>,
    fuel_quantity: Option<f64>,
}

impl From<WireCockpit> for Cockpit {
    fn from(v: WireCockpit) -> Self {
        Self {
            total_mileage_km: v.total_mileage,
            fuel_range_km: v.fuel_autonomy,
            fuel_quantity_liters: v.fuel_quantity,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WireLocation {
    last_update_time: Option<DateTime<Utc>>,
    gps_latitude: Option<f64>,
    gps_longitude: Option<f64>,
}

impl WireLocation {
    pub fn into_public(self) -> Result<Location> {
        if self
            .gps_latitude
            .is_some_and(|v| !(-90.0..=90.0).contains(&v))
            || self
                .gps_longitude
                .is_some_and(|v| !(-180.0..=180.0).contains(&v))
        {
            return Err(RenaultError::InvalidResponse {
                service: Service::Kamereon,
                context: "GPS coordinates out of range",
            });
        }
        Ok(Location {
            updated_at: self.last_update_time,
            latitude: self.gps_latitude,
            longitude: self.gps_longitude,
        })
    }
}
