//! Optional SMTP mail configuration ([`MailConfig`]) and its typed projections.

use lettre::{Address, message::Mailbox};
use secrecy::{ExposeSecret, SecretString};

use super::{ConfigError, EnvProvider};
use crate::services::mail::crypto::QueueKey;

/// How the connection to the relay is protected.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    serde::Serialize,
    serde::Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum SmtpSecurity {
    /// TLS from the first byte (SMTPS, default port 465).
    #[default]
    ImplicitTls,
    /// Plaintext greeting upgraded with a required STARTTLS (default port 587);
    /// a relay that does not offer it fails the submission.
    Starttls,
    /// No transport encryption (default port 25). Credentials and messages
    /// are exposed to anyone on the network path.
    Plaintext,
}

impl SmtpSecurity {
    /// The port used when `REVERIE_SMTP_PORT` is unset.
    #[must_use]
    pub const fn default_port(self) -> u16 {
        match self {
            Self::ImplicitTls => 465,
            Self::Starttls => 587,
            Self::Plaintext => 25,
        }
    }
}

/// Optional SMTP delivery settings. Nothing here is read or validated beyond
/// its syntax unless `enabled` is `true`.
#[derive(Clone, Default, serde::Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct MailConfig {
    /// Whether authentication mail is sent (`REVERIE_SMTP_ENABLED`, default
    /// `false`). When `false`, no queue key or mail credential file is read.
    pub enabled: bool,
    /// Relay host name or IP address (`REVERIE_SMTP_HOST`). Required when
    /// enabled; credentials and URL syntax are rejected.
    pub host: Option<String>,
    /// Relay port (`REVERIE_SMTP_PORT`). Defaults to 465 for `implicit_tls`,
    /// 587 for `starttls` and 25 for `plaintext`.
    pub port: Option<u16>,
    /// Transport protection (`REVERIE_SMTP_SECURITY`, default `implicit_tls`).
    /// Certificates are validated against the platform trust store; install a
    /// private CA there. `plaintext` is an explicit operator choice for a
    /// trusted connection and allows relay authentication.
    pub security: SmtpSecurity,
    /// Relay login (`REVERIE_SMTP_USERNAME`). Requires the password.
    pub username: Option<String>,
    /// Relay password (`REVERIE_SMTP_PASSWORD`, or a file through
    /// `REVERIE_SMTP_PASSWORD_FILE`). Requires the username; never logged.
    #[schemars(with = "Option<String>", default = "Option::<String>::default")]
    pub password: Option<SecretString>,
    /// Sender address (`REVERIE_SMTP_FROM`). Required when enabled.
    pub from: Option<String>,
    /// Support address used as Reply-To and in notices (`REVERIE_SMTP_REPLY_TO`).
    /// Required when enabled.
    pub reply_to: Option<String>,
    /// Queue encryption key (`REVERIE_MAIL_QUEUE_KEY`, or a file through
    /// `REVERIE_MAIL_QUEUE_KEY_FILE`): 32 random bytes as base64url without
    /// padding. Required when enabled; never logged.
    #[schemars(with = "Option<String>", default = "Option::<String>::default")]
    pub queue_key: Option<SecretString>,
}

impl std::fmt::Debug for MailConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MailConfig")
            .field("enabled", &self.enabled)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("security", &self.security)
            .field("username", &self.username.as_ref().map(|_| "<redacted>"))
            .field("password", &self.password)
            .field("from", &self.from.as_ref().map(|_| "<redacted>"))
            .field("reply_to", &self.reply_to.as_ref().map(|_| "<redacted>"))
            .field("queue_key", &self.queue_key)
            .finish()
    }
}

/// Relay login. Neither field appears in `Debug` output.
#[derive(Clone)]
pub struct RelayCredentials {
    username: String,
    password: SecretString,
}

impl RelayCredentials {
    /// Pair a relay login with its password.
    #[must_use]
    pub const fn new(username: String, password: SecretString) -> Self {
        Self { username, password }
    }

    /// The relay login name.
    #[must_use]
    pub fn username(&self) -> &str {
        &self.username
    }

    /// The relay password.
    #[must_use]
    pub fn password(&self) -> &str {
        self.password.expose_secret()
    }
}

impl std::fmt::Debug for RelayCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RelayCredentials(<redacted>)")
    }
}

/// Validated relay, sender and support settings: everything a submission
/// needs and nothing a queue needs.
#[derive(Debug, Clone)]
pub struct MailTransportConfig {
    /// Relay host, also the name the certificate must match.
    pub host: String,
    /// Relay port.
    pub port: u16,
    /// Transport protection.
    pub security: SmtpSecurity,
    /// Optional relay login.
    pub credentials: Option<RelayCredentials>,
    /// Envelope and header sender.
    pub from: Mailbox,
    /// Reply-To address; also the support address quoted in notices.
    pub reply_to: Mailbox,
}

impl MailTransportConfig {
    /// Load only the transport, sender and support settings from the process
    /// environment. The database, the queue key and every unrelated setting
    /// are never read, and `REVERIE_SMTP_ENABLED` is not consulted: a relay
    /// can be tried before sending is switched on.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] naming the offending setting when a mail
    /// setting is missing or invalid, or when a credential file cannot be read.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_provider(EnvProvider::from_process_env())
    }

    pub(crate) fn from_provider<R: Fn(&str) -> std::io::Result<String>>(
        provider: EnvProvider<R>,
    ) -> Result<Self, ConfigError> {
        use figment::Figment;

        let figment = Figment::from(provider.mail_transport_only());
        let only: MailOnly = figment
            .extract()
            .map_err(|error| super::map_figment_error(&error))?;
        only.mail.transport()
    }
}

#[derive(Default, serde::Deserialize)]
#[serde(default)]
struct MailOnly {
    mail: MailConfig,
}

const MAX_HOST_LEN: usize = 253;
const MAX_LABEL_LEN: usize = 63;

fn invalid(var: &str, reason: &str) -> ConfigError {
    ConfigError::Invalid {
        var: var.into(),
        reason: reason.into(),
    }
}

fn required<'a>(var: &str, value: Option<&'a String>) -> Result<&'a str, ConfigError> {
    value
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ConfigError::MissingVar(var.into()))
}

fn is_host_label(label: &str) -> bool {
    !label.is_empty()
        && label.len() <= MAX_LABEL_LEN
        && !label.starts_with('-')
        && !label.ends_with('-')
        && label
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

fn is_relay_host(host: &str) -> bool {
    host.parse::<std::net::IpAddr>().is_ok()
        || (host.len() <= MAX_HOST_LEN && host.split('.').all(is_host_label))
}

fn mailbox(var: &str, value: &str) -> Result<Mailbox, ConfigError> {
    const REASON: &str = "must be a plain email address such as support@example.com";
    if !crate::models::user::is_addr_spec(value) {
        return Err(invalid(var, REASON));
    }
    let address = value.parse::<Address>().map_err(|_| invalid(var, REASON))?;
    Ok(Mailbox::new(None, address))
}

impl MailConfig {
    /// Validate the relay, sender and support settings and project them into
    /// their typed form.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] naming the first missing or invalid setting.
    pub fn transport(&self) -> Result<MailTransportConfig, ConfigError> {
        let host = required("REVERIE_SMTP_HOST", self.host.as_ref())?;
        if !is_relay_host(host) {
            return Err(invalid(
                "REVERIE_SMTP_HOST",
                "must be a host name or IP address without credentials, scheme, port or path",
            ));
        }
        if self.port == Some(0) {
            return Err(invalid("REVERIE_SMTP_PORT", "must be between 1 and 65535"));
        }
        let credentials = match (&self.username, &self.password) {
            (None, None) => None,
            (Some(username), Some(password)) => {
                if username.is_empty() || username.chars().any(char::is_control) {
                    return Err(invalid(
                        "REVERIE_SMTP_USERNAME",
                        "must not be empty or contain control characters",
                    ));
                }
                Some(RelayCredentials {
                    username: username.clone(),
                    password: password.clone(),
                })
            }
            (Some(_), None) => {
                return Err(invalid(
                    "REVERIE_SMTP_PASSWORD",
                    "required when REVERIE_SMTP_USERNAME is set",
                ));
            }
            (None, Some(_)) => {
                return Err(invalid(
                    "REVERIE_SMTP_USERNAME",
                    "required when REVERIE_SMTP_PASSWORD is set",
                ));
            }
        };
        let from = mailbox(
            "REVERIE_SMTP_FROM",
            required("REVERIE_SMTP_FROM", self.from.as_ref())?,
        )?;
        let reply_to = mailbox(
            "REVERIE_SMTP_REPLY_TO",
            required("REVERIE_SMTP_REPLY_TO", self.reply_to.as_ref())?,
        )?;
        Ok(MailTransportConfig {
            host: host.to_owned(),
            port: self.port.unwrap_or_else(|| self.security.default_port()),
            security: self.security,
            credentials,
            from,
            reply_to,
        })
    }

    /// Decode the queue encryption key.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] when the key is absent or is not exactly 32
    /// bytes in canonical unpadded base64url. The value is never echoed.
    pub fn queue_key(&self) -> Result<QueueKey, ConfigError> {
        let secret = self
            .queue_key
            .as_ref()
            .ok_or_else(|| ConfigError::MissingVar("REVERIE_MAIL_QUEUE_KEY".into()))?;
        QueueKey::parse(secret.expose_secret()).map_err(|_| {
            invalid(
                "REVERIE_MAIL_QUEUE_KEY",
                "must be 32 random bytes encoded as base64url without padding",
            )
        })
    }

    /// Check an enabled configuration as the server needs it.
    pub(crate) fn validate_enabled(&self) -> Result<(), ConfigError> {
        self.transport()?;
        self.queue_key()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    const KEY: &str = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
    const BASE: &[(&str, &str)] = &[
        ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
        ("REVERIE_OPDS_ENABLED", "false"),
    ];

    fn enabled_vars() -> Vec<(&'static str, &'static str)> {
        let mut vars = BASE.to_vec();
        vars.extend([
            ("REVERIE_SMTP_ENABLED", "true"),
            ("REVERIE_SMTP_HOST", "smtp.example.com"),
            ("REVERIE_SMTP_FROM", "reverie@example.com"),
            ("REVERIE_SMTP_REPLY_TO", "support@example.com"),
            ("REVERIE_MAIL_QUEUE_KEY", KEY),
            ("REVERIE_PUBLIC_URL", "https://reverie.example.com"),
        ]);
        vars
    }

    fn load(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
        Config::from_figment(&figment::Figment::from(EnvProvider::from_pairs(vars)))
    }

    fn with(extra: &[(&'static str, &'static str)]) -> Vec<(&'static str, &'static str)> {
        let mut vars = enabled_vars();
        for &(key, value) in extra {
            vars.retain(|(existing, _)| *existing != key);
            vars.push((key, value));
        }
        vars
    }

    fn without(keys: &[&str]) -> Vec<(&'static str, &'static str)> {
        let mut vars = enabled_vars();
        vars.retain(|(key, _)| !keys.contains(key));
        vars
    }

    fn error_var(error: &ConfigError) -> String {
        match error {
            ConfigError::MissingVar(var) | ConfigError::Invalid { var, .. } => var.clone(),
            ConfigError::Multiple(errors) => {
                errors.iter().map(error_var).collect::<Vec<_>>().join(",")
            }
        }
    }

    #[test]
    fn smtp_config_disabled_by_default_without_any_mail_settings() {
        let config = load(BASE).unwrap();
        assert!(!config.mail.enabled);
        assert!(config.mail.host.is_none());
        assert!(config.mail.queue_key.is_none());
        assert!(config.public_url.is_none());
    }

    #[test]
    fn smtp_config_enabled_parses_with_implicit_tls_defaults() {
        let config = load(&enabled_vars()).unwrap();
        let transport = config.mail.transport().unwrap();
        assert_eq!(transport.host, "smtp.example.com");
        assert_eq!(transport.port, 465);
        assert_eq!(transport.security, SmtpSecurity::ImplicitTls);
        assert!(transport.credentials.is_none());
        assert_eq!(transport.from.email.to_string(), "reverie@example.com");
        assert_eq!(transport.reply_to.email.to_string(), "support@example.com");
        assert!(config.mail.queue_key().is_ok());
    }

    #[test]
    fn smtp_config_default_port_follows_security_mode() {
        for (mode, port) in [("implicit_tls", 465), ("starttls", 587), ("plaintext", 25)] {
            let config = load(&with(&[("REVERIE_SMTP_SECURITY", mode)])).unwrap();
            assert_eq!(config.mail.transport().unwrap().port, port, "{mode}");
        }
        let config = load(&with(&[
            ("REVERIE_SMTP_SECURITY", "starttls"),
            ("REVERIE_SMTP_PORT", "2525"),
        ]))
        .unwrap();
        assert_eq!(config.mail.transport().unwrap().port, 2525);
    }

    #[test]
    fn smtp_config_rejects_unknown_security_zero_port_and_bad_host() {
        for (key, value) in [
            ("REVERIE_SMTP_SECURITY", "tls"),
            ("REVERIE_SMTP_PORT", "0"),
            ("REVERIE_SMTP_PORT", "70000"),
            ("REVERIE_SMTP_HOST", "user:pass@smtp.example.com"),
            ("REVERIE_SMTP_HOST", "smtps://smtp.example.com"),
            ("REVERIE_SMTP_HOST", "smtp.example.com:465"),
            ("REVERIE_SMTP_HOST", "smtp.example.com/path"),
            ("REVERIE_SMTP_HOST", "bad host"),
            ("REVERIE_SMTP_HOST", "-bad.example.com"),
        ] {
            let error = load(&with(&[(key, value)])).unwrap_err();
            assert!(error_var(&error).contains(key), "{key}={value}: {error}");
            if key == "REVERIE_SMTP_HOST" {
                assert!(!error.to_string().contains(value), "{key} echoed its value");
            }
        }
    }

    #[test]
    fn smtp_config_accepts_ip_literal_host() {
        for host in ["127.0.0.1", "::1"] {
            let config = load(&with(&[("REVERIE_SMTP_HOST", host)])).unwrap();
            assert_eq!(config.mail.transport().unwrap().host, host);
        }
    }

    #[test]
    fn smtp_config_enabled_requires_each_setting_by_name() {
        for key in [
            "REVERIE_SMTP_HOST",
            "REVERIE_SMTP_FROM",
            "REVERIE_SMTP_REPLY_TO",
            "REVERIE_MAIL_QUEUE_KEY",
            "REVERIE_PUBLIC_URL",
        ] {
            let error = load(&without(&[key])).unwrap_err();
            assert!(error_var(&error).contains(key), "{key}: {error}");
        }
    }

    #[test]
    fn smtp_config_disabled_ignores_missing_and_unread_secrets() {
        let reads = std::cell::RefCell::new(Vec::new());
        let provider = EnvProvider::from_pairs(&[
            ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
            ("REVERIE_OPDS_ENABLED", "false"),
            ("REVERIE_SMTP_ENABLED", "false"),
            ("REVERIE_MAIL_QUEUE_KEY_FILE", "/queue-key-path"),
            ("REVERIE_SMTP_PASSWORD_FILE", "/password-path"),
            ("REVERIE_SMTP_HOST", "not a host"),
        ])
        .with_reader(|path: &str| {
            reads.borrow_mut().push(path.to_owned());
            Ok("unused".to_owned())
        });
        let config = Config::from_figment(&figment::Figment::from(provider)).unwrap();
        assert!(reads.borrow().is_empty(), "read {:?}", reads.borrow());
        assert!(config.mail.queue_key.is_none());
        assert!(config.mail.password.is_none());
    }

    #[test]
    fn smtp_config_absent_enabled_flag_never_reads_secret_files() {
        let provider = EnvProvider::from_pairs(&[
            ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
            ("REVERIE_OPDS_ENABLED", "false"),
            ("REVERIE_MAIL_QUEUE_KEY_FILE", "/queue-key-path"),
        ])
        .with_reader(|_: &str| -> std::io::Result<String> { panic!("must not read") });
        assert!(Config::from_figment(&figment::Figment::from(provider)).is_ok());
    }

    #[test]
    fn smtp_config_enabled_reads_secret_files_and_strips_terminators() {
        let provider = EnvProvider::from_pairs(&[
            ("DATABASE_URL", "postgres://test@localhost/reverie_dev"),
            ("REVERIE_OPDS_ENABLED", "false"),
            ("REVERIE_SMTP_ENABLED", "true"),
            ("REVERIE_SMTP_HOST", "smtp.example.com"),
            ("REVERIE_SMTP_FROM", "reverie@example.com"),
            ("REVERIE_SMTP_REPLY_TO", "support@example.com"),
            ("REVERIE_SMTP_USERNAME", "relay-user"),
            ("REVERIE_SMTP_PASSWORD_FILE", "/password-path"),
            ("REVERIE_MAIL_QUEUE_KEY_FILE", "/queue-key-path"),
            ("REVERIE_PUBLIC_URL", "https://reverie.example.com"),
        ])
        .with_reader(|path: &str| match path {
            "/password-path" => Ok("relay-pass\r\n".to_owned()),
            "/queue-key-path" => Ok(format!("{KEY}\n")),
            _ => Err(std::io::Error::other("unexpected path")),
        });
        let config = Config::from_figment(&figment::Figment::from(provider)).unwrap();
        let transport = config.mail.transport().unwrap();
        let credentials = transport.credentials.unwrap();
        assert_eq!(credentials.username(), "relay-user");
        assert_eq!(credentials.password(), "relay-pass");
        assert!(config.mail.queue_key().is_ok());
    }

    #[test]
    fn smtp_config_secret_source_conflict_names_both_settings_without_values() {
        for (name, direct, alias) in [
            (
                "REVERIE_MAIL_QUEUE_KEY",
                "queue-direct-marker",
                "REVERIE_MAIL_QUEUE_KEY_FILE",
            ),
            (
                "REVERIE_SMTP_PASSWORD",
                "password-direct-marker",
                "REVERIE_SMTP_PASSWORD_FILE",
            ),
        ] {
            let error =
                load(&with(&[(name, direct), (alias, "/conflict-path-marker")])).unwrap_err();
            let diagnostic = format!("{error} {error:?}");
            assert!(diagnostic.contains(name) && diagnostic.contains(alias));
            assert!(!diagnostic.contains(direct));
            assert!(!diagnostic.contains("/conflict-path-marker"));
        }
    }

    #[test]
    fn smtp_config_unreadable_secret_file_names_setting_without_path_or_cause() {
        let provider = EnvProvider::from_pairs(&with(&[
            ("REVERIE_MAIL_QUEUE_KEY", ""),
            ("REVERIE_MAIL_QUEUE_KEY_FILE", "/path-marker"),
        ]))
        .with_reader(|_: &str| Err(std::io::Error::other("io-marker")));
        let error = Config::from_figment(&figment::Figment::from(provider)).unwrap_err();
        let diagnostic = format!("{error} {error:?}");
        assert!(diagnostic.contains("REVERIE_MAIL_QUEUE_KEY"));
        assert!(!diagnostic.contains("path-marker"));
        assert!(!diagnostic.contains("io-marker"));
    }

    #[test]
    fn smtp_config_queue_key_must_be_canonical_32_byte_base64url() {
        let too_short = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh";
        let too_long = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8A";
        let padded = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=";
        let standard = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh+";
        let non_canonical = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh9";
        for bad in [
            too_short,
            too_long,
            padded,
            standard,
            non_canonical,
            "short",
            " ",
        ] {
            let error = load(&with(&[("REVERIE_MAIL_QUEUE_KEY", bad)])).unwrap_err();
            assert!(
                error_var(&error).contains("REVERIE_MAIL_QUEUE_KEY"),
                "{bad:?}: {error}"
            );
            assert!(!error.to_string().contains(bad.trim()) || bad.trim().is_empty());
        }
        assert!(load(&with(&[("REVERIE_MAIL_QUEUE_KEY", KEY)])).is_ok());
    }

    #[test]
    fn smtp_config_credentials_must_be_paired() {
        let error = load(&with(&[("REVERIE_SMTP_USERNAME", "relay-user")])).unwrap_err();
        assert!(error_var(&error).contains("REVERIE_SMTP_PASSWORD"));
        let error = load(&with(&[("REVERIE_SMTP_PASSWORD", "relay-pass")])).unwrap_err();
        assert!(error_var(&error).contains("REVERIE_SMTP_USERNAME"));
        let config = load(&with(&[
            ("REVERIE_SMTP_SECURITY", "plaintext"),
            ("REVERIE_SMTP_USERNAME", "relay-user"),
            ("REVERIE_SMTP_PASSWORD", "relay-pass"),
        ]))
        .unwrap();
        assert!(config.mail.transport().unwrap().credentials.is_some());
    }

    #[test]
    fn smtp_config_addresses_reject_headers_display_names_and_injection() {
        for (key, bad) in [
            ("REVERIE_SMTP_FROM", "Reverie <reverie@example.com>"),
            (
                "REVERIE_SMTP_FROM",
                "reverie@example.com\r\nBcc: x@example.com",
            ),
            ("REVERIE_SMTP_FROM", "reverie@example.com,other@example.com"),
            ("REVERIE_SMTP_REPLY_TO", "not-an-address"),
            ("REVERIE_SMTP_REPLY_TO", "support@[127.0.0.1]"),
            (
                "REVERIE_SMTP_REPLY_TO",
                "support@example.com\nBcc: x@example.com",
            ),
        ] {
            let error = load(&with(&[(key, bad)])).unwrap_err();
            assert!(error_var(&error).contains(key), "{key}: {error}");
        }
    }

    #[test]
    fn smtp_config_debug_never_shows_secrets_or_addresses() {
        let config = load(&with(&[
            ("REVERIE_SMTP_USERNAME", "username-marker"),
            ("REVERIE_SMTP_PASSWORD", "password-marker"),
        ]))
        .unwrap();
        let diagnostic = format!(
            "{:?} {:?} {:?}",
            config.mail,
            config,
            config.mail.transport().unwrap()
        );
        for marker in ["username-marker", "password-marker", KEY] {
            assert!(!diagnostic.contains(marker), "{marker} leaked");
        }
    }

    #[test]
    fn smtp_config_schema_defaults_leave_secrets_empty() {
        let schema = serde_json::to_value(schemars::schema_for!(MailConfig)).unwrap();
        for secret in ["password", "queue_key"] {
            let default = &schema["properties"][secret]["default"];
            assert!(default.is_null(), "{secret} default is {default}");
        }
        assert_eq!(schema["properties"]["enabled"]["default"], false);
        assert_eq!(schema["properties"]["security"]["default"], "implicit_tls");
    }

    #[test]
    fn smtp_config_transport_loader_reads_only_mail_transport_settings() {
        let reads = std::cell::RefCell::new(Vec::new());
        let provider = EnvProvider::from_pairs(&[
            ("DATABASE_URL_FILE", "/database-path"),
            ("OIDC_CLIENT_SECRET_FILE", "/oidc-path"),
            ("REVERIE_MAIL_QUEUE_KEY_FILE", "/queue-key-path"),
            ("REVERIE_PORT", "not-a-port"),
            ("REVERIE_SMTP_HOST", "smtp.example.com"),
            ("REVERIE_SMTP_FROM", "reverie@example.com"),
            ("REVERIE_SMTP_REPLY_TO", "support@example.com"),
            ("REVERIE_SMTP_USERNAME", "relay-user"),
            ("REVERIE_SMTP_PASSWORD_FILE", "/password-path"),
        ])
        .with_reader(|path: &str| {
            reads.borrow_mut().push(path.to_owned());
            Ok("relay-pass\n".to_owned())
        });
        let transport = MailTransportConfig::from_provider(provider).unwrap();
        assert_eq!(reads.borrow().as_slice(), ["/password-path"]);
        assert_eq!(transport.credentials.unwrap().password(), "relay-pass");
        assert_eq!(transport.port, 465);
    }

    #[test]
    fn smtp_config_transport_loader_names_missing_setting() {
        let provider = EnvProvider::from_pairs(&[
            ("REVERIE_SMTP_HOST", "smtp.example.com"),
            ("REVERIE_SMTP_FROM", "reverie@example.com"),
        ]);
        let error = MailTransportConfig::from_provider(provider).unwrap_err();
        assert!(
            error_var(&error).contains("REVERIE_SMTP_REPLY_TO"),
            "{error}"
        );
        let provider = EnvProvider::from_pairs(&[("REVERIE_SMTP_PORT", "abc")]);
        let error = MailTransportConfig::from_provider(provider).unwrap_err();
        assert!(error_var(&error).contains("REVERIE_SMTP_PORT"), "{error}");
    }

    #[test]
    fn smtp_config_public_origin_accepts_origins_and_normalises_root_slash() {
        for (input, expected) in [
            (
                "https://reverie.example.com",
                "https://reverie.example.com/",
            ),
            (
                "https://reverie.example.com/",
                "https://reverie.example.com/",
            ),
            ("http://localhost:3000", "http://localhost:3000/"),
        ] {
            let config = load(&with(&[("REVERIE_PUBLIC_URL", input)])).unwrap();
            assert_eq!(
                config.public_url.as_ref().map(url::Url::as_str),
                Some(expected)
            );
        }
    }

    #[test]
    fn smtp_config_public_origin_rejects_non_origins_without_echoing() {
        for bad in [
            "ftp://reverie.example.com",
            "https://user:pass@reverie.example.com",
            "https://reverie.example.com/library",
            "https://reverie.example.com/?next=1",
            "https://reverie.example.com/#frag",
            "reverie.example.com",
        ] {
            let error = load(&with(&[("REVERIE_PUBLIC_URL", bad)])).unwrap_err();
            assert!(
                error_var(&error).contains("REVERIE_PUBLIC_URL"),
                "{bad}: {error}"
            );
            assert!(
                !error.to_string().contains("pass@"),
                "{bad} echoed credentials"
            );
        }
    }

    #[test]
    fn smtp_config_public_origin_still_required_for_opds() {
        let mut vars = BASE.to_vec();
        vars.retain(|(key, _)| *key != "REVERIE_OPDS_ENABLED");
        let error = load(&vars).unwrap_err();
        assert!(error_var(&error).contains("REVERIE_PUBLIC_URL"));
        assert!(error.to_string().contains("REVERIE_OPDS_ENABLED"));
        vars.push(("REVERIE_PUBLIC_URL", "https://reverie.example.com"));
        let config = load(&vars).unwrap();
        assert!(config.opds.enabled);
    }
}
