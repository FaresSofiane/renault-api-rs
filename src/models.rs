//! Public response types. Missing values are preserved as `None`, never zero.
//! Unknown server fields are ignored; unknown plug/charge codes are retained.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

mod vehicle;
pub use vehicle::*;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Account {
    pub id: String,
    pub account_type: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Vehicle {
    pub vin: String,
    pub status: Option<String>,
    pub details: Option<VehicleDetails>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct CodeLabel {
    pub code: Option<String>,
    pub label: Option<String>,
    pub group: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct VehicleDetails {
    pub vin: Option<String>,
    pub registration_number: Option<String>,
    pub radio_code: Option<String>,
    pub brand: Option<CodeLabel>,
    pub model: Option<CodeLabel>,
    pub energy: Option<CodeLabel>,
    pub engine_energy_type: Option<String>,
    pub assets: Vec<Asset>,
}

impl VehicleDetails {
    pub fn uses_electricity(&self) -> bool {
        matches!(
            self.engine_energy_type
                .as_deref()
                .or_else(|| self.energy.as_ref()?.code.as_deref()),
            Some("ELEC" | "ELECX" | "PHEV")
        )
    }

    pub fn uses_fuel(&self) -> bool {
        matches!(
            self.engine_energy_type
                .as_deref()
                .or_else(|| self.energy.as_ref()?.code.as_deref()),
            Some("OTHER" | "PHEV" | "HEV")
        )
    }
    pub fn model_code(&self) -> Option<&str> {
        self.model.as_ref()?.code.as_deref()
    }

    pub fn picture_url(&self) -> Option<&str> {
        let asset = self
            .assets
            .iter()
            .find(|asset| asset.asset_type.as_deref() == Some("PICTURE"))?;
        asset
            .renditions
            .iter()
            .find(|r| r.resolution_type.as_deref() == Some("ONE_MYRENAULT_LARGE"))
            .or_else(|| asset.renditions.iter().find(|r| r.url.is_some()))?
            .url
            .as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Asset {
    pub asset_type: Option<String>,
    pub role: Option<String>,
    pub title: Option<String>,
    pub renditions: Vec<Rendition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Rendition {
    pub resolution_type: Option<String>,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct BatteryStatus {
    pub updated_at: Option<DateTime<Utc>>,
    pub level_percent: Option<u8>,
    pub temperature_celsius: Option<f64>,
    pub range_km: Option<f64>,
    pub plug_status: Option<PlugStatus>,
    pub charging_status: Option<ChargingStatus>,
    /// Normalized to kW. ZOE phase 1 (X101VE) reports watts.
    pub charging_power_kw: Option<f64>,
    pub charging_remaining_minutes: Option<u32>,
    /// This estimate can have a different timestamp from the battery reading.
    pub charging_remaining_updated_at: Option<DateTime<Utc>>,
    /// Preserved upstream values; the reference does not establish reliable units.
    pub capacity_raw: Option<f64>,
    pub available_energy_raw: Option<f64>,
    pub v2l_system_status: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[non_exhaustive]
pub enum PlugStatus {
    Unplugged,
    Plugged,
    Error,
    Unavailable,
    Unknown(i32),
}

impl From<i32> for PlugStatus {
    fn from(value: i32) -> Self {
        match value {
            0 => Self::Unplugged,
            1 => Self::Plugged,
            -1 => Self::Error,
            i32::MIN => Self::Unavailable,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize, Serialize)]
#[non_exhaustive]
pub enum ChargingStatus {
    NotCharging,
    WaitingForPlannedCharge,
    Ended,
    WaitingForCurrentCharge,
    EnergyFlapOpen,
    Charging,
    /// The same code means "not charging" on older ZOEs and "error" on ZE50.
    ErrorOrNotCharging,
    Unavailable,
    V2gWaiting,
    V2lConnected,
    V2gDischarging,
    V2gCharging,
    Unknown(f64),
}

impl From<f64> for ChargingStatus {
    fn from(value: f64) -> Self {
        match value {
            0.0 => Self::NotCharging,
            0.1 => Self::WaitingForPlannedCharge,
            0.2 => Self::Ended,
            0.3 => Self::WaitingForCurrentCharge,
            0.4 => Self::EnergyFlapOpen,
            1.0 => Self::Charging,
            -1.0 => Self::ErrorOrNotCharging,
            -1.1 => Self::Unavailable,
            -1.3 => Self::V2gWaiting,
            -1.4 => Self::V2lConnected,
            -1.5 => Self::V2gDischarging,
            -1.6 => Self::V2gCharging,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Cockpit {
    pub total_mileage_km: Option<f64>,
    pub fuel_range_km: Option<f64>,
    pub fuel_quantity_liters: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Location {
    pub updated_at: Option<DateTime<Utc>>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}
