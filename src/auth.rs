use std::{
    fmt,
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::{RenaultError, Result};

/// Authentication inputs are deliberately redacted from Debug output.
pub enum Auth {
    Credentials {
        email: String,
        password: String,
    },
    SessionToken(String),
    /// A JWT alone cannot mint a new access token. `personId` must be in its
    /// payload to use `accounts()`; account/VIN calls do not require this claim.
    AccessToken(String),
    RestoredSession(SessionCredentials),
}

impl fmt::Debug for Auth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Credentials { .. } => "Credentials([REDACTED])",
            Self::SessionToken(_) => "SessionToken([REDACTED])",
            Self::AccessToken(_) => "AccessToken([REDACTED])",
            Self::RestoredSession(_) => "RestoredSession([REDACTED])",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthOutcome {
    Authenticated,
    TwoFactorRequired,
}

/// Gigya device cookies. Explicitly export/restore them to remember a device
/// across process restarts. This crate never persists them.
#[derive(Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIds {
    pub gmid: Option<String>,
    pub ucid: Option<String>,
}

impl fmt::Debug for DeviceIds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceIds([REDACTED])")
    }
}

impl DeviceIds {
    pub(crate) fn pairs(&self) -> Vec<(&str, &str)> {
        [
            ("gmid", self.gmid.as_deref()),
            ("ucid", self.ucid.as_deref()),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k, v)))
        .collect()
    }

    pub(crate) fn cookie_header(&self) -> Option<String> {
        let pairs = self.pairs();
        (!pairs.is_empty()).then(|| {
            pairs
                .into_iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ")
        })
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if self.pairs().iter().any(|(_, v)| {
            v.is_empty()
                || !v.bytes().all(|b| {
                    b == b'!'
                        || (b'#'..=b'+').contains(&b)
                        || (b'-'..=b':').contains(&b)
                        || (b'<'..=b'[').contains(&b)
                        || (b']'..=b'~').contains(&b)
                })
        }) {
            return Err(RenaultError::InvalidInput(
                "invalid Gigya device cookie".into(),
            ));
        }
        Ok(())
    }
}

/// Explicit session export. Serialization is the caller's choice; no password,
/// JWT or pending challenge is included. Treat this value as a credential.
#[derive(Clone, Serialize, Deserialize)]
pub struct SessionCredentials {
    pub login_token: String,
    pub device_ids: DeviceIds,
}

impl fmt::Debug for SessionCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionCredentials([REDACTED])")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct TwoFactorEmail {
    pub id: String,
    pub obfuscated: Option<String>,
}

pub(crate) struct PendingAuth {
    pub email: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub reg_token: Zeroizing<String>,
    pub assertion: Option<Zeroizing<String>>,
    pub phv_token: Option<Zeroizing<String>>,
    pub provider_assertion: Option<Zeroizing<String>>,
    pub finalized: bool,
}

pub(crate) struct AccessToken {
    pub value: Zeroizing<String>,
    pub expires_at: u64,
    pub person_id: Option<String>,
}

impl AccessToken {
    pub fn parse(value: String) -> Result<Self> {
        let value = Zeroizing::new(value);
        let parts: Vec<_> = value.split('.').collect();
        if parts.len() != 3 || parts.iter().any(|part| part.is_empty()) {
            return Err(RenaultError::InvalidAccessToken);
        }
        // This is expiry/claim inspection only, NOT signature verification.
        // The remote service is responsible for validating the access token.
        let decoded = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| RenaultError::InvalidAccessToken)?;
        #[derive(Deserialize)]
        struct Claims {
            exp: u64,
            #[serde(rename = "personId")]
            person_id: Option<String>,
            data: Option<PersonClaims>,
        }
        #[derive(Deserialize)]
        struct PersonClaims {
            #[serde(rename = "personId")]
            person_id: Option<String>,
        }
        let claims: Claims =
            serde_json::from_slice(&decoded).map_err(|_| RenaultError::InvalidAccessToken)?;
        Ok(Self {
            value,
            expires_at: claims.exp,
            person_id: claims
                .person_id
                .or_else(|| claims.data.and_then(|d| d.person_id)),
        })
    }

    pub fn is_fresh(&self) -> bool {
        self.expires_at > now().saturating_add(60)
    }
}

pub(crate) fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
