use super::*;

fn relays(config: &Config) -> Vec<RelayConfig> {
    let RelayMode::Custom(map) = &config.relay_mode else {
        panic!("expected a custom relay mode");
    };
    map.relays::<Vec<_>>()
        .into_iter()
        .map(|relay| (*relay).clone())
        .collect()
}

#[test]
fn parses_a_relay_with_a_token() {
    let config = Config::parse(
        r#"
        bind_port = 0

        [[relay]]
        url = "https://relay.example.com"
        auth_token = "secret"
        "#,
    )
    .unwrap();
    let relays = relays(&config);
    assert_eq!(relays.len(), 1);
    assert_eq!(relays[0].auth_token.as_deref(), Some("secret"));
    // QUIC address discovery stays on its default port.
    assert_eq!(relays[0].quic.as_ref().unwrap().port, 7842);
}

#[test]
fn auth_token_is_optional() {
    let config = Config::parse(
        r#"
        bind_port = 0

        [[relay]]
        url = "https://relay.example.com"
        "#,
    )
    .unwrap();
    assert_eq!(relays(&config)[0].auth_token, None);
}

#[test]
fn parses_a_custom_quic_port() {
    let config = Config::parse(
        r#"
        bind_port = 0

        [[relay]]
        url = "https://relay.example.com"
        quic = { port = 9999 }
        "#,
    )
    .unwrap();
    assert_eq!(relays(&config)[0].quic.as_ref().unwrap().port, 9999);
}

/// The key path is hardcoded, so a config that tries to move it must fail
/// loudly rather than be ignored while the key stays where it was.
#[test]
fn rejects_a_secret_key_path() {
    assert!(
        Config::parse(
            r#"
            bind_port = 0
            secret_key = "keys/receiver.key"

            [[relay]]
            url = "https://relay.example.com"
            "#,
        )
        .is_err()
    );
}

#[test]
fn rejects_a_missing_bind_port() {
    let err = Config::parse(
        r#"
        [[relay]]
        url = "https://relay.example.com"
        "#,
    )
    .unwrap_err();
    assert!(
        format!("{err:#}").contains("bind_port"),
        "unexpected error: {err:#}"
    );
}

#[test]
fn parses_a_bind_port() {
    let config = Config::parse(
        r#"
        bind_port = 41999

        [[relay]]
        url = "https://relay.example.com"
        "#,
    )
    .unwrap();
    assert_eq!(config.bind_port, 41999);
}

#[test]
fn allows_a_zero_bind_port_for_an_ephemeral_one() {
    let config = Config::parse(
        r#"
        bind_port = 0

        [[relay]]
        url = "https://relay.example.com"
        "#,
    )
    .unwrap();
    assert_eq!(config.bind_port, 0);
}

#[test]
fn rejects_an_empty_config() {
    assert!(Config::parse("   ").is_err());
}

#[test]
fn rejects_a_config_without_relays() {
    assert!(
        Config::parse(
            r#"
            bind_port = 0
            relay = []
            "#
        )
        .is_err()
    );
}

#[test]
fn rejects_an_empty_auth_token() {
    assert!(
        Config::parse(
            r#"
            bind_port = 0

            [[relay]]
            url = "https://relay.example.com"
            auth_token = ""
            "#,
        )
        .is_err()
    );
}

#[test]
fn rejects_a_duplicate_relay() {
    assert!(
        Config::parse(
            r#"
            bind_port = 0

            [[relay]]
            url = "https://relay.example.com"
            auth_token = "a"

            [[relay]]
            url = "https://relay.example.com"
            auth_token = "b"
            "#,
        )
        .is_err()
    );
}

#[test]
fn rejects_an_invalid_url() {
    assert!(
        Config::parse(
            r#"
            bind_port = 0

            [[relay]]
            url = "not a url"
            "#,
        )
        .is_err()
    );
}

/// The download directory is hardcoded, so a config that tries to move it must
/// fail loudly rather than be ignored while pushes keep landing in `downloads`.
#[test]
fn rejects_a_download_dir() {
    assert!(
        Config::parse(
            r#"
            bind_port = 0
            download_dir = "/srv/incoming"

            [[relay]]
            url = "https://relay.example.com"
            "#,
        )
        .is_err()
    );
}

#[test]
fn rejects_an_unknown_top_level_key() {
    assert!(
        Config::parse(
            r#"
            bind_port = 0
            nope = true

            [[relay]]
            url = "https://relay.example.com"
            "#,
        )
        .is_err()
    );
}
