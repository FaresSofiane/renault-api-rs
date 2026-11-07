use std::time::Duration;

use reqwest::Url;

use crate::{RenaultError, Result};

const EU_GIGYA_KEY: &str = "3_VgdkgtIRH3AdHvJm-cjV2ug2EFE0lxt0IJzMC4MFqZjFpn_GYFXVdNZ19L7wZX0N";
const KAMEREON_KEY: &str = "YjkKtHmGfaceeuExUDKGxrLZGGvtVS0J";

/// API configuration from the reference Python client. These are public app
/// keys, not user credentials. Custom endpoints allow updating them independently.
#[derive(Clone)]
pub struct Endpoints {
    pub(crate) gigya_url: Url,
    pub(crate) gigya_key: String,
    pub(crate) kamereon_url: Url,
    pub(crate) kamereon_key: String,
}

impl Endpoints {
    pub fn gigya_url(&self) -> &str {
        self.gigya_url.as_str()
    }
    pub fn gigya_key(&self) -> &str {
        &self.gigya_key
    }
    pub fn kamereon_url(&self) -> &str {
        self.kamereon_url.as_str()
    }
    pub fn kamereon_key(&self) -> &str {
        &self.kamereon_key
    }
    /// Roots must be HTTPS (HTTP is accepted only on loopback for local tests).
    pub fn new(
        gigya_url: &str,
        gigya_key: impl Into<String>,
        kamereon_url: &str,
        kamereon_key: impl Into<String>,
    ) -> Result<Self> {
        let endpoints = Self {
            gigya_url: root_url(gigya_url)?,
            gigya_key: gigya_key.into(),
            kamereon_url: root_url(kamereon_url)?,
            kamereon_key: kamereon_key.into(),
        };
        if endpoints.gigya_key.is_empty() || endpoints.kamereon_key.is_empty() {
            return Err(RenaultError::Configuration(
                "API keys cannot be empty".into(),
            ));
        }
        Ok(endpoints)
    }
}

/// Explicit locale/country and a bounded network timeout. No filesystem access.
#[derive(Clone)]
pub struct Config {
    pub(crate) locale: String,
    pub(crate) country: String,
    pub(crate) endpoints: Endpoints,
    pub(crate) timeout: Duration,
}

impl Config {
    pub fn locale(&self) -> &str {
        &self.locale
    }
    pub fn country(&self) -> &str {
        &self.country
    }
    pub fn endpoints(&self) -> &Endpoints {
        &self.endpoints
    }
    pub fn timeout(&self) -> Duration {
        self.timeout
    }
    /// Explicitly fetch public regional API settings from Renault. Requires Tokio;
    /// this never reads user credentials and does not persist the result.
    pub async fn discover(locale: &str) -> Result<Self> {
        Self::discover_from(
            locale,
            "https://renault-wrd-prod-1-euw1-myrapp-one.s3-eu-west-1.amazonaws.com/",
        )
        .await
    }

    async fn discover_from(locale: &str, root: &str) -> Result<Self> {
        let country = validate_locale(locale)?;
        let root = root_url(root)?;
        let url = root
            .join(&format!("configuration/android/config_{locale}.json"))
            .map_err(|_| RenaultError::Configuration("invalid configuration URL".into()))?;
        let http = crate::client::http_client(Duration::from_secs(30))?;
        let (status, value) =
            crate::client::read_json(http.get(url).send().await?, crate::Service::Configuration)
                .await?;
        if !(200..300).contains(&status) {
            return Err(RenaultError::Http {
                service: crate::Service::Configuration,
                status,
            });
        }
        #[derive(serde::Deserialize)]
        struct Server {
            apikey: String,
            target: String,
        }
        #[derive(serde::Deserialize)]
        struct Servers {
            #[serde(rename = "gigyaProd")]
            gigya: Server,
            #[serde(rename = "wiredProd")]
            wired: Server,
        }
        #[derive(serde::Deserialize)]
        struct Document {
            servers: Servers,
        }
        let document: Document = crate::wire::decode(value, crate::Service::Configuration)?;
        let endpoints = Endpoints::new(
            &document.servers.gigya.target,
            document.servers.gigya.apikey,
            &document.servers.wired.target,
            document.servers.wired.apikey,
        )?;
        Self::with_endpoints(locale, country, endpoints)
    }
    /// Use a locale supported by the Python reference, e.g. `fr_FR`.
    /// Unknown locales require [`Self::with_endpoints`], rather than silently
    /// choosing a potentially incorrect authentication region.
    pub fn new(locale: &str) -> Result<Self> {
        let country = validate_locale(locale)?;
        let (region, gigya_url, gigya_key) = match locale {
            "en_AU" => (
                "apn1",
                "https://gigya-prod-au1.renaultgroup.com",
                "4_GC9iGd3sH8sMJpz2x0q0CQ",
            ),
            "es_MX" => (
                "usw2",
                "https://accounts.us1.gigya.com",
                "4_yTFqPSsGxVyRXPZUM7t1Iw",
            ),
            "fr_LU" => (
                "euw1",
                "https://accounts.eu1.gigya.com",
                "3_zt44Wl_wT9mnqn-BHrR19PvXj3wYRPQKLcPbGWawlatFR837KdxSZZStbBTDaqnb",
            ),
            "bg_BG" | "cs_CZ" | "da_DK" | "de_DE" | "de_AT" | "de_CH" | "en_GB" | "en_IE"
            | "es_ES" | "fi_FI" | "fr_FR" | "fr_BE" | "fr_CH" | "hr_HR" | "it_IT" | "it_CH"
            | "nl_NL" | "nl_BE" | "no_NO" | "pl_PL" | "pt_PT" | "ro_RO" | "ru_RU" | "sk_SK"
            | "sl_SI" | "sv_SE" | "hu_HU" => {
                ("euw1", "https://accounts.eu1.gigya.com", EU_GIGYA_KEY)
            }
            _ => {
                return Err(RenaultError::Configuration(format!(
                    "unsupported locale {locale}; supply explicit endpoints"
                )));
            }
        };
        let endpoints = Endpoints::new(
            gigya_url,
            gigya_key,
            &format!("https://api-wired-prod-1-{region}.wrd-aws.com"),
            KAMEREON_KEY,
        )?;
        Self::with_endpoints(locale, country, endpoints)
    }

    pub fn with_endpoints(locale: &str, country: &str, endpoints: Endpoints) -> Result<Self> {
        validate_locale(locale)?;
        if country.len() != 2 || !country.bytes().all(|b| b.is_ascii_uppercase()) {
            return Err(RenaultError::Configuration(
                "country must be two uppercase ASCII letters".into(),
            ));
        }
        Ok(Self {
            locale: locale.into(),
            country: country.into(),
            endpoints,
            timeout: Duration::from_secs(30),
        })
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self> {
        if timeout.is_zero() {
            return Err(RenaultError::Configuration(
                "timeout must be greater than zero".into(),
            ));
        }
        self.timeout = timeout;
        Ok(self)
    }
}

fn validate_locale(locale: &str) -> Result<&str> {
    let b = locale.as_bytes();
    if b.len() != 5
        || b[2] != b'_'
        || !b[..2].iter().all(u8::is_ascii_lowercase)
        || !b[3..].iter().all(u8::is_ascii_uppercase)
    {
        return Err(RenaultError::Configuration(
            "locale must use ll_CC, e.g. fr_FR".into(),
        ));
    }
    Ok(&locale[3..])
}

fn root_url(input: &str) -> Result<Url> {
    let mut url = Url::parse(input)
        .map_err(|_| RenaultError::Configuration("invalid API root URL".into()))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"));
    if !(url.scheme() == "https" || url.scheme() == "http" && loopback)
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(RenaultError::Configuration(
            "API roots require HTTPS without credentials, query or fragment".into(),
        ));
    }
    if !url.path().ends_with('/') {
        url.set_path(&format!("{}/", url.path()));
    }
    Ok(url)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{method, path},
    };
    #[tokio::test]
    async fn discovery_accepts_a_new_locale_and_ignores_incorrect_content_type() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/configuration/android/config_xx_XX.json"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"servers":{
                        "gigyaProd":{"apikey":"gigya-key","target":"https://gigya.example.test"},
                        "wiredProd":{"apikey":"wired-key","target":"https://wired.example.test"}
                    }}))
                    .insert_header("content-type", "application/octet-stream"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let config = Config::discover_from("xx_XX", &server.uri()).await.unwrap();
        assert_eq!(config.country, "XX");
        assert_eq!(config.endpoints.gigya_key, "gigya-key");
    }
}
