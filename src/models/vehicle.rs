//! Additional readings, schedules, histories and command acknowledgements.
use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{RenaultError, Result};

pub type ExtraFields = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct BatterySoc {
    #[serde(alias = "lastEnergyUpdateTimestamp")]
    pub updated_at: Option<DateTime<Utc>>,
    pub soc_min: Option<u8>,
    pub soc_target: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TyrePressure {
    #[serde(alias = "flPressure")]
    pub front_left_raw: Option<f64>,
    #[serde(alias = "frPressure")]
    pub front_right_raw: Option<f64>,
    #[serde(alias = "rlPressure")]
    pub rear_left_raw: Option<f64>,
    #[serde(alias = "rrPressure")]
    pub rear_right_raw: Option<f64>,
    #[serde(alias = "flStatus")]
    pub front_left_status: Option<i32>,
    #[serde(alias = "frStatus")]
    pub front_right_status: Option<i32>,
    #[serde(alias = "rlStatus")]
    pub rear_left_status: Option<i32>,
    #[serde(alias = "rrStatus")]
    pub rear_right_status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HvacStatus {
    #[serde(alias = "lastUpdateTime")]
    pub updated_at: Option<DateTime<Utc>>,
    #[serde(alias = "externalTemperature")]
    pub external_temperature_celsius: Option<f64>,
    #[serde(alias = "internalTemperature")]
    pub internal_temperature_celsius: Option<f64>,
    #[serde(alias = "hvacStatus")]
    pub status: Option<String>,
    #[serde(alias = "nextHvacStartDate")]
    pub next_start_at: Option<DateTime<Utc>>,
    #[serde(alias = "socThreshold")]
    pub soc_threshold_percent: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LockStatus {
    #[serde(alias = "lockStatus")]
    pub status: Option<String>,
    pub rear_left: Option<String>,
    pub rear_right: Option<String>,
    pub driver: Option<String>,
    pub passenger: Option<String>,
    pub hatch: Option<String>,
    #[serde(alias = "lastUpdateTime")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResState {
    pub code: Option<String>,
    pub details: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct CarAdapter {
    pub vin: Option<String>,
    pub vehicle_id: Option<u64>,
    pub battery_code: Option<String>,
    pub brand: Option<String>,
    pub can_generation: Option<String>,
    pub car_gateway: Option<String>,
    pub delivery_country: Option<String>,
    pub delivery_date: Option<String>,
    pub energy: Option<String>,
    pub engine_type: Option<String>,
    pub family_code: Option<String>,
    pub first_registration_date: Option<String>,
    pub gearbox: Option<String>,
    pub model_code: Option<String>,
    pub model_code_detail: Option<String>,
    pub model_name: Option<String>,
    pub radio_type: Option<String>,
    pub region: Option<String>,
    pub registration_country: Option<String>,
    pub registration_number: Option<String>,
    pub tcu_code: Option<String>,
    pub version_code: Option<String>,
    pub privacy_mode: Option<String>,
    pub privacy_mode_update_date: Option<DateTime<Utc>>,
    pub svt_flag: Option<bool>,
    pub svt_block_flag: Option<bool>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

impl CarAdapter {
    pub fn uses_electricity(&self) -> bool {
        self.energy.as_deref() == Some("electric")
    }
    pub fn uses_fuel(&self) -> bool {
        self.energy.as_deref() == Some("gasoline")
    }
    pub fn reports_power_in_watts(&self) -> bool {
        self.car_gateway.as_deref() == Some("GDC")
    }
}

/// UTC weekly time used by KCA schedules, serialized as `Thh:mmZ`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ScheduleTime(String);

impl ScheduleTime {
    pub fn new(hour: u8, minute: u8) -> Result<Self> {
        if hour > 23 || minute > 59 {
            return Err(RenaultError::InvalidInput(
                "invalid UTC schedule time".into(),
            ));
        }
        Ok(Self(format!("T{hour:02}:{minute:02}Z")))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn end_time(&self, duration_minutes: u16) -> Self {
        let bytes = self.0.as_bytes();
        let hours = (bytes[1] - b'0') as u32 * 10 + (bytes[2] - b'0') as u32;
        let minutes = (bytes[4] - b'0') as u32 * 10 + (bytes[5] - b'0') as u32;
        let end = (hours * 60 + minutes + u32::from(duration_minutes)) % 1440;
        Self(format!("T{:02}:{:02}Z", end / 60, end % 60))
    }
}
impl TryFrom<String> for ScheduleTime {
    type Error = RenaultError;
    fn try_from(value: String) -> Result<Self> {
        let b = value.as_bytes();
        if b.len() != 7
            || b[0] != b'T'
            || b[3] != b':'
            || b[6] != b'Z'
            || ![b[1], b[2], b[4], b[5]].iter().all(u8::is_ascii_digit)
        {
            return Err(RenaultError::InvalidInput(
                "schedule time must use Thh:mmZ".into(),
            ));
        }
        Self::new(
            (b[1] - b'0') * 10 + b[2] - b'0',
            (b[4] - b'0') * 10 + b[5] - b'0',
        )
    }
}
impl From<ScheduleTime> for String {
    fn from(value: ScheduleTime) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChargeDay {
    pub start_time: Option<ScheduleTime>,
    #[serde(rename = "duration")]
    pub duration_minutes: Option<u16>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HvacDay {
    pub ready_at_time: Option<ScheduleTime>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct WeeklySchedule<D> {
    pub id: Option<u32>,
    pub activated: Option<bool>,
    pub monday: Option<D>,
    pub tuesday: Option<D>,
    pub wednesday: Option<D>,
    pub thursday: Option<D>,
    pub friday: Option<D>,
    pub saturday: Option<D>,
    pub sunday: Option<D>,
}
pub type ChargeSchedule = WeeklySchedule<ChargeDay>;
pub type HvacSchedule = WeeklySchedule<HvacDay>;

impl<D> WeeklySchedule<D> {
    pub fn days(&self) -> [Option<&D>; 7] {
        [
            self.monday.as_ref(),
            self.tuesday.as_ref(),
            self.wednesday.as_ref(),
            self.thursday.as_ref(),
            self.friday.as_ref(),
            self.saturday.as_ref(),
            self.sunday.as_ref(),
        ]
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargeMode {
    #[serde(alias = "chargeMode")]
    pub mode: Option<String>,
}

/// Command strings differ from the strings returned by the charge-mode reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargeModeRequest {
    Always,
    Scheduled,
}
impl ChargeModeRequest {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Always => "always_charging",
            Self::Scheduled => "schedule_mode",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct ChargingSettings {
    pub mode: Option<String>,
    pub schedules: Option<Vec<ChargeSchedule>>,
    #[serde(alias = "startDateTime")]
    pub start_at: Option<DateTime<Utc>>,
    #[serde(alias = "dateTime")]
    pub updated_at: Option<DateTime<Utc>>,
    pub delay: Option<u32>,
}

impl ChargingSettings {
    pub fn upsert_schedule(&mut self, schedule: ChargeSchedule) -> Result<()> {
        upsert(self.schedules.get_or_insert_default(), schedule)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HvacSettings {
    pub mode: Option<String>,
    pub schedules: Option<Vec<HvacSchedule>>,
    #[serde(alias = "dateTime")]
    pub updated_at: Option<DateTime<Utc>>,
}
impl HvacSettings {
    pub fn upsert_schedule(&mut self, schedule: HvacSchedule) -> Result<()> {
        upsert(self.schedules.get_or_insert_default(), schedule)
    }
}
fn upsert<D>(schedules: &mut Vec<WeeklySchedule<D>>, schedule: WeeklySchedule<D>) -> Result<()> {
    let id = schedule
        .id
        .ok_or_else(|| RenaultError::InvalidInput("schedule ID is required".into()))?;
    if let Some(existing) = schedules.iter_mut().find(|s| s.id == Some(id)) {
        *existing = schedule;
    } else {
        schedules.push(schedule);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct CalendarSlot {
    pub start_time: Option<String>,
    pub duration: Option<u32>,
    pub activation_state: Option<bool>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargeCalendar {
    pub calendar: BTreeMap<String, Vec<CalendarSlot>>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvProgram {
    pub program_activation_status: Option<bool>,
    pub program_type: Option<String>,
    pub program_departure_time: Option<String>,
    pub program_activation_monday: Option<bool>,
    pub program_activation_tuesday: Option<bool>,
    pub program_activation_wednesday: Option<bool>,
    pub program_activation_thursday: Option<bool>,
    pub program_activation_friday: Option<bool>,
    pub program_activation_saturday: Option<bool>,
    pub program_activation_sunday: Option<bool>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvSettings {
    pub last_settings_update_timestamp: Option<DateTime<Utc>>,
    pub delegated_activated: Option<bool>,
    pub charge_mode_rq: Option<String>,
    pub charge_time_start: Option<String>,
    pub charge_duration: Option<u32>,
    pub preconditioning_temperature: Option<f64>,
    pub preconditioning_heated_strg_wheel: Option<bool>,
    pub preconditioning_heated_right_seat: Option<bool>,
    pub preconditioning_heated_left_seat: Option<bool>,
    pub programs: Option<Vec<EvProgram>>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChargeScheduleData {
    Calendar(ChargeCalendar),
    EvSettings(EvSettings),
}

/// No schema is established by the reference fixtures for these responses.
/// Preserve all attributes instead of inventing types or discarding data.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AdditionalData {
    #[serde(flatten)]
    pub fields: ExtraFields,
}
pub type NotificationSettings = AdditionalData;
pub type HvacHistory = AdditionalData;
pub type HvacSessions = AdditionalData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HistoryPeriod {
    Day,
    Month,
}
impl HistoryPeriod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Month => "month",
        }
    }
    pub(crate) fn date_format(self) -> &'static str {
        match self {
            Self::Day => "%Y%m%d",
            Self::Month => "%Y%m",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DurationUnit {
    Minutes,
    Seconds,
}
impl DurationUnit {
    pub fn for_model(model: Option<&str>) -> Self {
        if model == Some("X101VE") {
            Self::Minutes
        } else {
            Self::Seconds
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct ChargeSummary {
    pub day: Option<String>,
    pub month: Option<String>,
    pub total_charges_number: Option<u32>,
    pub total_charges_duration: Option<f64>,
    pub total_charges_errors: Option<u32>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChargeHistory {
    pub summaries: Option<Vec<ChargeSummary>>,
    pub duration_unit: DurationUnit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct ChargeSession {
    pub charge_start_date: Option<DateTime<Utc>>,
    pub charge_end_date: Option<DateTime<Utc>>,
    pub charge_duration: Option<f64>,
    pub charge_start_battery_level: Option<u8>,
    pub charge_end_battery_level: Option<u8>,
    pub charge_battery_level_recovered: Option<f64>,
    pub charge_power: Option<String>,
    pub charge_start_instantaneous_power: Option<f64>,
    pub charge_energy_recovered: Option<f64>,
    pub charge_end_status: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Charges {
    pub charges: Option<Vec<ChargeSession>>,
    pub duration_unit: DurationUnit,
    pub instantaneous_power_in_watts: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct Contract {
    #[serde(alias = "type")]
    pub contract_type: Option<String>,
    pub contract_id: Option<String>,
    pub code: Option<String>,
    pub group: Option<String>,
    pub duration_months: Option<u32>,
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub status: Option<String>,
    pub status_label: Option<String>,
    pub description: Option<String>,
    pub conditions: Option<Vec<ContractCondition>>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct ContractCondition {
    pub start_date: Option<NaiveDate>,
    pub end_date: Option<NaiveDate>,
    pub unlimited_mileage: Option<bool>,
    pub maximum_mileage: Option<f64>,
    pub mileage_unit: Option<String>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all(deserialize = "camelCase"))]
pub struct Alert {
    pub id: Option<String>,
    pub alert_type: Option<String>,
    pub alert_code: Option<String>,
    pub vin: Option<String>,
    pub short_description: Option<String>,
    pub long_description: Option<String>,
    pub criticity: Option<i32>,
    pub status: Option<i32>,
    pub creation_date_time: Option<DateTime<Utc>>,
    pub last_update_date_time: Option<DateTime<Utc>>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}

/// An HTTP acknowledgement, not proof that the vehicle completed the action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandReceipt {
    pub id: Option<String>,
    pub kind: Option<String>,
    pub command_id: Option<String>,
    pub status: Option<String>,
    pub attributes: CommandAttributes,
    pub ev_settings: Option<EvSettings>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CommandAttributes {
    pub action: Option<String>,
    pub target: Option<String>,
    #[serde(alias = "targetTemperature")]
    pub target_temperature_celsius: Option<f64>,
    #[serde(alias = "startDateTime")]
    pub start_at: Option<DateTime<Utc>>,
    pub charge_schedules: Option<Vec<ChargeSchedule>>,
    pub hvac_schedules: Option<Vec<HvacSchedule>>,
    #[serde(flatten)]
    pub extra: ExtraFields,
}
