use std::fmt;

/// Result returned by all fallible operations in this crate.
pub type Result<T> = std::result::Result<T, RenaultError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Service {
    Gigya,
    Kamereon,
}

impl fmt::Display for Service {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Stable classification; unknown upstream codes remain available in `Api`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ApiErrorKind {
    AccessDenied,
    NotSupported,
    NotFound,
    QuotaExceeded,
    PrivacyMode,
    InvalidInput,
    Upstream,
    Other,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RenaultError {
    #[error("invalid configuration: {0}")]
    Configuration(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("authenticate the client first")]
    NotAuthenticated,
    #[error("authentication expired or was revoked; authenticate again")]
    AuthenticationExpired,
    #[error("incorrect email or password")]
    InvalidCredentials,
    #[error("no email two-factor authentication is pending")]
    NoPendingTwoFactor,
    #[error("request a verification code before completing two-factor authentication")]
    VerificationCodeNotRequested,
    #[error("incorrect verification code; the challenge can be retried")]
    InvalidVerificationCode,
    #[error("choose an email address before requesting a verification code")]
    EmailSelectionRequired,
    #[error("no email address is available for two-factor authentication")]
    NoTwoFactorEmail,
    #[error("invalid JWT: expected an exp claim and a valid payload")]
    InvalidAccessToken,
    #[error("{endpoint} is not supported for vehicle model {model_code}")]
    UnsupportedEndpoint {
        endpoint: &'static str,
        model_code: String,
    },
    #[error("{service} returned HTTP {status}")]
    Http { service: Service, status: u16 },
    // Intentionally omit upstream bodies/messages: auth assertions and personal
    // data can appear in them. Preserve the code, classification and status.
    #[error("{service} API error {code} ({kind:?}, HTTP {status})")]
    Api {
        service: Service,
        code: String,
        kind: ApiErrorKind,
        status: u16,
    },
    #[error("invalid {service} response: {context}")]
    InvalidResponse {
        service: Service,
        context: &'static str,
    },
    #[error("HTTP transport failed")]
    Transport(#[source] reqwest::Error),
}

impl From<reqwest::Error> for RenaultError {
    fn from(error: reqwest::Error) -> Self {
        // TFA URLs contain assertions and codes in their query string.
        Self::Transport(error.without_url())
    }
}
