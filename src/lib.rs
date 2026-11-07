//! A standalone, asynchronous Renault API client with no persistence or UI.
//!
//! Use [`RenaultApi::authenticate`] with credentials, a session token, or an
//! access token. The async client requires a caller-provided Tokio runtime.
//! Clones share their in-memory session and connection pool.

#![doc = include_str!("../README.md")]

mod auth;
mod client;
mod config;
mod error;
pub mod models;
mod wire;

pub use auth::{Auth, AuthOutcome, DeviceIds, SessionCredentials, TwoFactorEmail};
pub use client::RenaultApi;
pub use config::{Config, Endpoints};
pub use error::{ApiErrorKind, RenaultError, Result, Service};

// Keep Markdown examples checked against the public API without executing real
// network calls. These modules are only collected by rustdoc's doctest pass.
#[cfg(doctest)]
mod documentation {
    #[doc = include_str!("../docs/demarrage.md")]
    mod demarrage {}
    #[doc = include_str!("../docs/authentification.md")]
    mod authentification {}
    #[doc = include_str!("../docs/reference-api.md")]
    mod reference_api {}
    #[doc = include_str!("../docs/modeles.md")]
    mod modeles {}
    #[doc = include_str!("../docs/configuration.md")]
    mod configuration {}
    #[doc = include_str!("../docs/erreurs.md")]
    mod erreurs {}
    #[doc = include_str!("../docs/architecture.md")]
    mod architecture {}
    #[doc = include_str!("../docs/plateformes.md")]
    mod plateformes {}
    #[doc = include_str!("../docs/developpement.md")]
    mod developpement {}
}
