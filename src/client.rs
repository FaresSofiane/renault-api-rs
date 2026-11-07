use std::{collections::HashMap, sync::Arc};

use reqwest::{
    Client, Method, RequestBuilder, Response, Url,
    header::{COOKIE, HeaderValue, SET_COOKIE},
};
use serde_json::Value;
use tokio::sync::Mutex;
use zeroize::Zeroizing;

use crate::{
    Auth, AuthOutcome, Config, DeviceIds, RenaultError, Result, Service, SessionCredentials,
    TwoFactorEmail,
    auth::{AccessToken, PendingAuth},
    models::*,
    wire,
};

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;

#[derive(Default)]
struct State {
    login_token: Option<Zeroizing<String>>,
    access_token: Option<AccessToken>,
    person_id: Option<String>,
    device_ids: DeviceIds,
    pending: Option<PendingAuth>,
    // Only vehicle model codes, for routing and unit conversion; no telemetry.
    models: HashMap<(String, String), Option<String>>,
}

struct Inner {
    http: Client,
    config: Config,
    state: Mutex<State>,
}

/// Shared, asynchronous Renault client. No implicit polling, disk access, or
/// retries. Clones share authentication; refreshes are serialized by a mutex.
#[derive(Clone)]
pub struct RenaultApi {
    inner: Arc<Inner>,
}

impl RenaultApi {
    /// Construct a client without network calls. Requires Tokio when used.
    /// TLS uses bundled Mozilla roots, avoiding mandatory Android JVM setup.
    /// Update dependencies to keep the root certificates current.
    pub fn new(config: Config) -> Result<Self> {
        let roots = rustls::RootCertStore {
            roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
        };
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| RenaultError::Configuration("cannot initialize TLS protocol versions".into()))?
        .with_root_certificates(roots)
        .with_no_client_auth();
        let http = Client::builder()
            .tls_backend_preconfigured(tls)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .timeout(config.timeout)
            .connect_timeout(config.timeout.min(std::time::Duration::from_secs(10)))
            .user_agent(concat!("renault-api-rust/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                config,
                state: Mutex::new(State::default()),
            }),
        })
    }

    /// Replace the current authentication, dropping any pending challenge and
    /// previously cached account/vehicle metadata. Device IDs are retained.
    pub async fn authenticate(&self, auth: Auth) -> Result<AuthOutcome> {
        let mut state = self.inner.state.lock().await;
        let device_ids = std::mem::take(&mut state.device_ids);
        *state = State {
            device_ids,
            ..State::default()
        };
        match auth {
            Auth::Credentials { email, password } => {
                self.login(&mut state, Zeroizing::new(email), Zeroizing::new(password))
                    .await
            }
            Auth::SessionToken(token) => {
                validate_nonempty(&token, "session token")?;
                state.login_token = Some(Zeroizing::new(token));
                self.ensure_access_token(&mut state).await?;
                Ok(AuthOutcome::Authenticated)
            }
            Auth::RestoredSession(session) => {
                validate_nonempty(&session.login_token, "session token")?;
                session.device_ids.validate()?;
                state.device_ids = session.device_ids;
                state.login_token = Some(Zeroizing::new(session.login_token));
                self.ensure_access_token(&mut state).await?;
                Ok(AuthOutcome::Authenticated)
            }
            Auth::AccessToken(token) => {
                let token = AccessToken::parse(token)?;
                if !token.is_fresh() {
                    return Err(RenaultError::AuthenticationExpired);
                }
                state.person_id = token.person_id.clone();
                state.access_token = Some(token);
                Ok(AuthOutcome::Authenticated)
            }
        }
    }

    /// Discard all local authentication, device IDs and metadata. Does not
    /// revoke the remote Gigya session. Other clones are also logged out.
    pub async fn logout(&self) {
        *self.inner.state.lock().await = State::default();
    }

    /// Drop pending TFA credentials/assertions, including the temporary password.
    pub async fn cancel_two_factor(&self) {
        self.inner.state.lock().await.pending = None;
    }

    /// Export only the reusable session token and remembered-device identifiers.
    /// JWT-only authentication has no reusable session and returns `None`.
    pub async fn session_credentials(&self) -> Option<SessionCredentials> {
        let state = self.inner.state.lock().await;
        Some(SessionCredentials {
            login_token: state.login_token.as_ref()?.to_string(),
            device_ids: state.device_ids.clone(),
        })
    }

    pub async fn device_ids(&self) -> DeviceIds {
        self.inner.state.lock().await.device_ids.clone()
    }

    /// Restore remembered-device identifiers BEFORE starting a password login.
    pub async fn set_device_ids(&self, device_ids: DeviceIds) -> Result<()> {
        device_ids.validate()?;
        let mut state = self.inner.state.lock().await;
        if state.pending.is_some() {
            return Err(RenaultError::InvalidInput(
                "cancel the pending challenge before replacing device IDs".into(),
            ));
        }
        state.device_ids = device_ids;
        Ok(())
    }

    pub async fn two_factor_emails(&self) -> Result<Vec<TwoFactorEmail>> {
        let mut state = self.inner.state.lock().await;
        self.tfa_emails(&mut state).await
    }

    /// Send a code. Omit `email_id` only when exactly one address is available.
    /// Sending a new code replaces the previous verification token.
    pub async fn request_two_factor_code(&self, email_id: Option<&str>) -> Result<()> {
        let mut state = self.inner.state.lock().await;
        let emails = self.tfa_emails(&mut state).await?;
        let id = match email_id {
            Some(id) if emails.iter().any(|email| email.id == id) => id.to_owned(),
            Some(_) => {
                return Err(RenaultError::InvalidInput(
                    "unknown two-factor email ID".into(),
                ));
            }
            None if emails.is_empty() => return Err(RenaultError::NoTwoFactorEmail),
            None if emails.len() > 1 => return Err(RenaultError::EmailSelectionRequired),
            None => emails[0].id.clone(),
        };
        let pending = state
            .pending
            .as_mut()
            .ok_or(RenaultError::NoPendingTwoFactor)?;
        if pending.provider_assertion.is_some() || pending.finalized {
            return Err(RenaultError::InvalidInput(
                "verification already completed; finish the pending login".into(),
            ));
        }
        // Invalidate a previous code before sending another one. If the server
        // accepted the request but the reply is lost, the old code is not reused.
        pending.phv_token = None;
        let pending = state
            .pending
            .as_ref()
            .ok_or(RenaultError::NoPendingTwoFactor)?;
        let assertion = pending
            .assertion
            .as_deref()
            .ok_or(RenaultError::NoPendingTwoFactor)?;
        let value = self
            .gigya(
                "accounts.tfa.email.sendVerificationCode",
                Method::GET,
                &[
                    ("gigyaAssertion", assertion.as_str()),
                    ("emailID", &id),
                    ("lang", &self.inner.config.locale[..2]),
                ],
                &state.device_ids,
                false,
                false,
            )
            .await?;
        let token = required_string(&value, "/phvToken", Service::Gigya)?;
        state
            .pending
            .as_mut()
            .ok_or(RenaultError::NoPendingTwoFactor)?
            .phv_token = Some(Zeroizing::new(token));
        Ok(())
    }

    /// A wrong code leaves the challenge available for retry. After successful
    /// verification, retries resume finalization/login without reusing the OTP.
    pub async fn complete_two_factor(
        &self,
        code: &str,
        remember_device: bool,
    ) -> Result<AuthOutcome> {
        validate_nonempty(code, "verification code")?;
        let mut state = self.inner.state.lock().await;
        let pending = state
            .pending
            .as_ref()
            .ok_or(RenaultError::NoPendingTwoFactor)?;
        let assertion = pending
            .assertion
            .as_ref()
            .ok_or(RenaultError::VerificationCodeNotRequested)?;
        let phv = pending
            .phv_token
            .as_ref()
            .ok_or(RenaultError::VerificationCodeNotRequested)?;
        if pending.provider_assertion.is_none() {
            let value = self
                .gigya(
                    "accounts.tfa.email.completeVerification",
                    Method::GET,
                    &[
                        ("gigyaAssertion", assertion.as_str()),
                        ("phvToken", phv.as_str()),
                        ("code", code),
                    ],
                    &state.device_ids,
                    false,
                    true,
                )
                .await?;
            let provider = required_string(&value, "/providerAssertion", Service::Gigya)?;
            state
                .pending
                .as_mut()
                .ok_or(RenaultError::NoPendingTwoFactor)?
                .provider_assertion = Some(Zeroizing::new(provider));
        }
        let pending = state
            .pending
            .as_ref()
            .ok_or(RenaultError::NoPendingTwoFactor)?;
        if !pending.finalized {
            let assertion = pending
                .assertion
                .as_ref()
                .ok_or(RenaultError::VerificationCodeNotRequested)?;
            let provider = pending
                .provider_assertion
                .as_ref()
                .ok_or(RenaultError::VerificationCodeNotRequested)?;
            self.gigya(
                "accounts.tfa.finalizeTFA",
                Method::GET,
                &[
                    ("gigyaAssertion", assertion.as_str()),
                    ("providerAssertion", provider.as_str()),
                    ("regToken", pending.reg_token.as_str()),
                    ("tempDevice", if remember_device { "false" } else { "true" }),
                ],
                &state.device_ids,
                false,
                false,
            )
            .await?;
            state
                .pending
                .as_mut()
                .ok_or(RenaultError::NoPendingTwoFactor)?
                .finalized = true;
        }
        let pending = state
            .pending
            .as_ref()
            .ok_or(RenaultError::NoPendingTwoFactor)?;
        let email = pending.email.clone();
        let password = pending.password.clone();
        self.login(&mut state, email, password).await
    }

    pub async fn accounts(&self) -> Result<Vec<Account>> {
        let person_id = {
            let mut state = self.inner.state.lock().await;
            self.ensure_access_token(&mut state).await?;
            if state.person_id.is_none() {
                let token = state.login_token.as_ref().ok_or_else(|| {
                    RenaultError::InvalidInput(
                        "JWT has no personId; use a session token to discover accounts".into(),
                    )
                })?;
                let value = self
                    .gigya(
                        "accounts.getAccountInfo",
                        Method::POST,
                        &[("login_token", token.as_str())],
                        &DeviceIds::default(),
                        false,
                        false,
                    )
                    .await;
                if matches!(value, Err(RenaultError::AuthenticationExpired)) {
                    clear_auth(&mut state);
                }
                state.person_id = Some(required_string(&value?, "/data/personId", Service::Gigya)?);
            }
            state
                .person_id
                .clone()
                .ok_or(RenaultError::NotAuthenticated)?
        };
        let value = self.get(&["commerce", "v1", "persons", &person_id]).await?;
        let person: wire::Person = wire::decode(value, Service::Kamereon)?;
        Ok(person
            .accounts
            .into_iter()
            .filter_map(|a| a.into_public())
            .collect())
    }

    pub async fn vehicles(&self, account_id: &str) -> Result<Vec<Vehicle>> {
        validate_identifier(account_id)?;
        let value = self
            .get(&["commerce", "v1", "accounts", account_id, "vehicles"])
            .await?;
        let response: wire::Vehicles = wire::decode(value, Service::Kamereon)?;
        let vehicles: Vec<_> = response
            .vehicle_links
            .into_iter()
            .filter_map(|v| v.into_public())
            .collect();
        let mut state = self.inner.state.lock().await;
        for vehicle in &vehicles {
            if let Some(details) = &vehicle.details {
                state.models.insert(
                    (account_id.into(), vehicle.vin.clone()),
                    details.model_code().map(str::to_owned),
                );
            } else {
                state
                    .models
                    .remove(&(account_id.into(), vehicle.vin.clone()));
            }
        }
        Ok(vehicles)
    }

    /// Always fetch fresh details; only the model code is retained in memory.
    pub async fn vehicle_details(&self, account_id: &str, vin: &str) -> Result<VehicleDetails> {
        validate_vehicle(account_id, vin)?;
        let value = self
            .get(&[
                "commerce", "v1", "accounts", account_id, "vehicles", vin, "details",
            ])
            .await?;
        let wire: wire::Details = wire::decode(value, Service::Kamereon)?;
        let details = VehicleDetails::from(wire);
        self.inner.state.lock().await.models.insert(
            (account_id.into(), vin.into()),
            details.model_code().map(str::to_owned),
        );
        Ok(details)
    }

    pub async fn battery_status(&self, account_id: &str, vin: &str) -> Result<BatteryStatus> {
        validate_vehicle(account_id, vin)?;
        let model = self.model_code(account_id, vin).await?;
        if matches!(
            model.as_deref(),
            Some(
                "BD11SL"
                    | "DU31SU"
                    | "XHN1ML"
                    | "XHN1SU"
                    | "XJA1VP"
                    | "XJA2VP"
                    | "XJB2CP"
                    | "XJB2SU"
                    | "XJF2BI"
            )
        ) {
            return Err(RenaultError::UnsupportedEndpoint {
                endpoint: "battery-status",
                model_code: model.unwrap_or_default(),
            });
        }
        let value = self
            .vehicle_data(account_id, vin, "v2", "battery-status")
            .await?;
        wire::attributes::<wire::Battery>(value)?.into_public(model.as_deref())
    }

    pub async fn cockpit(&self, account_id: &str, vin: &str) -> Result<Cockpit> {
        validate_vehicle(account_id, vin)?;
        let value = self.vehicle_data(account_id, vin, "v1", "cockpit").await?;
        Ok(wire::attributes::<wire::WireCockpit>(value)?.into())
    }

    pub async fn location(&self, account_id: &str, vin: &str) -> Result<Location> {
        validate_vehicle(account_id, vin)?;
        let model = self.model_code(account_id, vin).await?;
        if matches!(model.as_deref(), Some("X101VE" | "XFB2BI")) {
            return Err(RenaultError::UnsupportedEndpoint {
                endpoint: "location",
                model_code: model.unwrap_or_default(),
            });
        }
        let value = self.vehicle_data(account_id, vin, "v1", "location").await?;
        wire::attributes::<wire::WireLocation>(value)?.into_public()
    }

    async fn login(
        &self,
        state: &mut State,
        email: Zeroizing<String>,
        password: Zeroizing<String>,
    ) -> Result<AuthOutcome> {
        validate_nonempty(&email, "email")?;
        validate_nonempty(&password, "password")?;
        let value = self
            .gigya(
                "accounts.login",
                Method::POST,
                &[("loginID", email.as_str()), ("password", password.as_str())],
                &state.device_ids,
                true,
                false,
            )
            .await?;
        if value.get("errorCode").and_then(Value::as_i64) == Some(403101) {
            let reg_token = required_string(&value, "/regToken", Service::Gigya)?;
            state.pending = Some(PendingAuth {
                email,
                password,
                reg_token: Zeroizing::new(reg_token),
                assertion: None,
                phv_token: None,
                provider_assertion: None,
                finalized: false,
            });
            return Ok(AuthOutcome::TwoFactorRequired);
        }
        let token = required_string(&value, "/sessionInfo/cookieValue", Service::Gigya)?;
        state.login_token = Some(Zeroizing::new(token));
        state.pending = None;
        Ok(AuthOutcome::Authenticated)
    }

    async fn prepare_tfa(&self, state: &mut State) -> Result<()> {
        if state.pending.is_none() {
            return Err(RenaultError::NoPendingTwoFactor);
        }
        if state.device_ids.pairs().is_empty() {
            let url = url_for(
                &self.inner.config.endpoints.gigya_url,
                &["accounts.webSdkBootstrap"],
            )?;
            let response = self
                .inner
                .http
                .get(url)
                .query(&[("APIKey", &self.inner.config.endpoints.gigya_key)])
                .send()
                .await?;
            if !response.status().is_success() {
                return Err(RenaultError::Http {
                    service: Service::Gigya,
                    status: response.status().as_u16(),
                });
            }
            let mut ids = DeviceIds::default();
            for cookie in response.headers().get_all(SET_COOKIE) {
                if let Ok(cookie) = cookie.to_str()
                    && let Some((name, value)) =
                        cookie.split(';').next().and_then(|c| c.split_once('='))
                {
                    match name.trim() {
                        "gmid" => ids.gmid = Some(value.to_owned()),
                        "ucid" => ids.ucid = Some(value.to_owned()),
                        _ => {}
                    }
                }
            }
            ids.validate()?;
            if ids.pairs().is_empty() {
                return Err(RenaultError::InvalidResponse {
                    service: Service::Gigya,
                    context: "bootstrap returned no device identifiers",
                });
            }
            state.device_ids = ids;
        }
        let pending = state
            .pending
            .as_ref()
            .ok_or(RenaultError::NoPendingTwoFactor)?;
        if pending.assertion.is_none() {
            let mut params = vec![
                ("regToken", pending.reg_token.as_str()),
                ("provider", "gigyaEmail"),
                ("mode", "verify"),
            ];
            params.extend(state.device_ids.pairs());
            let value = self
                .gigya(
                    "accounts.tfa.initTFA",
                    Method::GET,
                    &params,
                    &state.device_ids,
                    false,
                    false,
                )
                .await?;
            let assertion = required_string(&value, "/gigyaAssertion", Service::Gigya)?;
            state
                .pending
                .as_mut()
                .ok_or(RenaultError::NoPendingTwoFactor)?
                .assertion = Some(Zeroizing::new(assertion));
        }
        Ok(())
    }

    async fn tfa_emails(&self, state: &mut State) -> Result<Vec<TwoFactorEmail>> {
        self.prepare_tfa(state).await?;
        let assertion = state
            .pending
            .as_ref()
            .and_then(|p| p.assertion.as_ref())
            .ok_or(RenaultError::NoPendingTwoFactor)?;
        let value = self
            .gigya(
                "accounts.tfa.email.getEmails",
                Method::GET,
                &[("gigyaAssertion", assertion.as_str())],
                &state.device_ids,
                false,
                false,
            )
            .await?;
        #[derive(serde::Deserialize)]
        struct Email {
            id: Option<String>,
            obfuscated: Option<String>,
        }
        #[derive(serde::Deserialize)]
        struct Emails {
            emails: Vec<Email>,
        }
        let emails: Emails = wire::decode(value, Service::Gigya)?;
        Ok(emails
            .emails
            .into_iter()
            .filter_map(|e| {
                Some(TwoFactorEmail {
                    id: e.id.filter(|id| !id.is_empty())?,
                    obfuscated: e.obfuscated,
                })
            })
            .collect())
    }

    async fn ensure_access_token(&self, state: &mut State) -> Result<()> {
        if state
            .access_token
            .as_ref()
            .is_some_and(AccessToken::is_fresh)
        {
            return Ok(());
        }
        let expired_access_token = state.access_token.is_some();
        state.access_token = None;
        let session = state.login_token.as_ref().ok_or(if expired_access_token {
            RenaultError::AuthenticationExpired
        } else {
            RenaultError::NotAuthenticated
        })?;
        let value = self
            .gigya(
                "accounts.getJWT",
                Method::POST,
                &[
                    ("login_token", session.as_str()),
                    ("fields", "data.personId,data.gigyaDataCenter"),
                    ("expiration", "900"),
                ],
                &DeviceIds::default(),
                false,
                false,
            )
            .await;
        if matches!(value, Err(RenaultError::AuthenticationExpired)) {
            clear_auth(state);
        }
        let value = value?;
        let token = AccessToken::parse(required_string(&value, "/id_token", Service::Gigya)?)?;
        if !token.is_fresh() {
            return Err(RenaultError::AuthenticationExpired);
        }
        if state.person_id.is_none() {
            state.person_id = token.person_id.clone();
        }
        state.access_token = Some(token);
        Ok(())
    }

    async fn model_code(&self, account_id: &str, vin: &str) -> Result<Option<String>> {
        let key = (account_id.to_owned(), vin.to_owned());
        if let Some(model) = self.inner.state.lock().await.models.get(&key).cloned() {
            return Ok(model);
        }
        Ok(self
            .vehicle_details(account_id, vin)
            .await?
            .model_code()
            .map(str::to_owned))
    }

    async fn vehicle_data(
        &self,
        account_id: &str,
        vin: &str,
        version: &str,
        endpoint: &str,
    ) -> Result<Value> {
        self.get(&[
            "commerce",
            "v1",
            "accounts",
            account_id,
            "kamereon",
            "kca",
            "car-adapter",
            version,
            "cars",
            vin,
            endpoint,
        ])
        .await
    }

    async fn get(&self, path: &[&str]) -> Result<Value> {
        let token = {
            let mut state = self.inner.state.lock().await;
            self.ensure_access_token(&mut state).await?;
            state
                .access_token
                .as_ref()
                .ok_or(RenaultError::NotAuthenticated)?
                .value
                .clone()
        };
        let mut header =
            HeaderValue::from_str(&token).map_err(|_| RenaultError::InvalidAccessToken)?;
        header.set_sensitive(true);
        let url = url_for(&self.inner.config.endpoints.kamereon_url, path)?;
        let request = self
            .inner
            .http
            .get(url)
            .header("content-type", "application/vnd.api+json")
            .header("apikey", &self.inner.config.endpoints.kamereon_key)
            .header("x-gigya-id_token", header)
            .query(&[("country", &self.inner.config.country)]);
        let result = async {
            let (status, value) = read_json(request.send().await?, Service::Kamereon).await?;
            wire::check_error(&value, Service::Kamereon, status, false)?;
            check_http(status, Service::Kamereon)?;
            Ok(value)
        }
        .await;
        if matches!(result, Err(RenaultError::AuthenticationExpired)) {
            let mut state = self.inner.state.lock().await;
            if state
                .access_token
                .as_ref()
                .is_some_and(|t| t.value.as_str() == token.as_str())
            {
                state.access_token = None;
            }
        }
        result
    }

    async fn gigya(
        &self,
        endpoint: &str,
        method: Method,
        params: &[(&str, &str)],
        device_ids: &DeviceIds,
        allow_tfa: bool,
        verifying_code: bool,
    ) -> Result<Value> {
        let url = url_for(&self.inner.config.endpoints.gigya_url, &[endpoint])?;
        let mut request = self.inner.http.request(method.clone(), url);
        let mut params = params.to_vec();
        params.push((
            if method == Method::POST {
                "ApiKey"
            } else {
                "APIKey"
            },
            &self.inner.config.endpoints.gigya_key,
        ));
        request = if method == Method::POST {
            request.form(&params)
        } else {
            request.query(&params)
        };
        request = with_device_cookies(request, device_ids)?;
        let (status, value) = read_json(request.send().await?, Service::Gigya).await?;
        if allow_tfa && value.get("errorCode").and_then(Value::as_i64) == Some(403101) {
            return Ok(value);
        }
        wire::check_error(&value, Service::Gigya, status, verifying_code)?;
        check_http(status, Service::Gigya)?;
        Ok(value)
    }
}

fn clear_auth(state: &mut State) {
    state.login_token = None;
    state.access_token = None;
    state.person_id = None;
    state.models.clear();
}

fn with_device_cookies(request: RequestBuilder, device_ids: &DeviceIds) -> Result<RequestBuilder> {
    device_ids.validate()?;
    match device_ids.cookie_header() {
        Some(cookie) => {
            let mut value = HeaderValue::from_str(&cookie)
                .map_err(|_| RenaultError::InvalidInput("invalid Gigya device cookie".into()))?;
            value.set_sensitive(true);
            Ok(request.header(COOKIE, value))
        }
        None => Ok(request),
    }
}

fn check_http(status: u16, service: Service) -> Result<()> {
    if status == 401 {
        return Err(RenaultError::AuthenticationExpired);
    }
    if !(200..300).contains(&status) {
        return Err(RenaultError::Http { service, status });
    }
    Ok(())
}

async fn read_json(mut response: Response, service: Service) -> Result<(u16, Value)> {
    let status = response.status().as_u16();
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(RenaultError::InvalidResponse {
                service,
                context: "response exceeds 2 MiB limit",
            });
        }
        body.extend_from_slice(&chunk);
    }
    match serde_json::from_slice(&body) {
        Ok(value) => Ok((status, value)),
        Err(_) => {
            check_http(status, service)?;
            Err(RenaultError::InvalidResponse {
                service,
                context: "expected JSON",
            })
        }
    }
}

fn url_for(root: &Url, segments: &[&str]) -> Result<Url> {
    let mut url = root.clone();
    {
        let mut path = url.path_segments_mut().map_err(|()| {
            RenaultError::Configuration("API root cannot contain path segments".into())
        })?;
        path.pop_if_empty();
        for segment in segments {
            validate_identifier(segment)?;
            path.push(segment);
        }
    }
    Ok(url)
}

fn validate_nonempty(value: &str, name: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(RenaultError::InvalidInput(format!(
            "{name} cannot be empty"
        )));
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<()> {
    if value.is_empty() || matches!(value, "." | "..") || value.chars().any(char::is_control) {
        return Err(RenaultError::InvalidInput(
            "invalid account, person, VIN or endpoint identifier".into(),
        ));
    }
    Ok(())
}

fn validate_vehicle(account_id: &str, vin: &str) -> Result<()> {
    validate_identifier(account_id)?;
    validate_identifier(vin)
}

fn required_string(value: &Value, pointer: &str, service: Service) -> Result<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or(RenaultError::InvalidResponse {
            service,
            context: "missing required identifier or token",
        })
}
