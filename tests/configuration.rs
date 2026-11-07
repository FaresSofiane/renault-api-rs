#![allow(clippy::unwrap_used, clippy::expect_used)]

use renault_api::{Config, DeviceIds, Endpoints, RenaultApi};

#[test]
fn supported_locales_and_regions_have_valid_configuration() {
    for locale in ["fr_FR", "fr_LU", "en_AU", "es_MX", "de_DE", "it_CH"] {
        assert!(Config::new(locale).is_ok());
    }
    for locale in ["", "FR", "fr-fr", "FR_fr", "é_FR", "xx_XX"] {
        assert!(Config::new(locale).is_err());
    }
}

#[test]
fn custom_endpoints_require_safe_roots_but_allow_loopback_tests() {
    for url in [
        "http://example.com",
        "https://user:password@example.com",
        "https://example.com?key=secret",
        "https://example.com/#fragment",
        "file:///tmp/test",
    ] {
        assert!(Endpoints::new(url, "key", "https://example.com", "key").is_err());
    }
    assert!(Endpoints::new("http://127.0.0.1:1234/", "key", "http://[::1]:1234/", "key").is_ok());
    let endpoints = Endpoints::new(
        "https://example.com/base",
        "key",
        "https://example.com",
        "key",
    )
    .unwrap();
    assert!(Config::with_endpoints("xx_XX", "XX", endpoints.clone()).is_ok());
    assert!(Config::with_endpoints("fr_FR", "fr", endpoints).is_err());
    assert!(
        Config::new("fr_FR")
            .unwrap()
            .with_timeout(std::time::Duration::ZERO)
            .is_err()
    );
}

#[tokio::test]
async fn untrusted_device_cookie_values_cannot_inject_headers_or_cookies() {
    let api = RenaultApi::new(Config::new("fr_FR").unwrap()).unwrap();
    for cookie in [
        "foo; other=bar",
        "foo\r\nInjected: true",
        "quoted\"",
        "",
        "a b",
    ] {
        assert!(
            api.set_device_ids(DeviceIds {
                gmid: Some(cookie.into()),
                ucid: None
            })
            .await
            .is_err()
        );
    }
}

#[test]
fn client_is_send_sync_and_clone_for_native_integrations() {
    fn assert_traits<T: Send + Sync + Clone>() {}
    assert_traits::<RenaultApi>();
}
