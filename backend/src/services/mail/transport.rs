//! One bounded SMTP submission, shared by the queue worker and `send-test-email`.

use std::time::Duration;

use lettre::{
    Message,
    message::{Mailbox, header::ContentType},
    transport::smtp::{
        Error as SmtpError,
        authentication::{Credentials, Mechanism},
        client::{AsyncSmtpConnection, TlsParameters},
        extension::ClientId,
    },
};
use uuid::Uuid;

use super::FailureCategory;
use crate::config::{MailTransportConfig, SmtpSecurity};

/// Upper bound for the whole exchange: connect, TLS, login, send and quit.
pub const SUBMISSION_TIMEOUT: Duration = Duration::from_secs(30);

/// A plain-text message ready for submission.
#[derive(Debug, Clone)]
pub struct OutgoingMail {
    /// The single envelope and header recipient.
    pub recipient: Mailbox,
    /// The subject line.
    pub subject: String,
    /// The plain-text body.
    pub body: String,
    /// When set, derives a stable Message-ID so retries of one job share it.
    pub job_id: Option<Uuid>,
}

#[derive(Clone, Copy)]
enum Phase {
    Connect,
    StartTls,
    Auth,
    Send,
}

fn is_tls_handshake_failure(error: &SmtpError) -> bool {
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if cause
            .downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.kind() == std::io::ErrorKind::InvalidData)
        {
            return true;
        }
        source = cause.source();
    }
    false
}

fn categorise(error: &SmtpError, phase: Phase) -> FailureCategory {
    if error.is_timeout() {
        return FailureCategory::Timeout;
    }
    if error.is_tls() {
        return FailureCategory::Tls;
    }
    match phase {
        Phase::Connect => {
            if is_tls_handshake_failure(error) {
                FailureCategory::Tls
            } else {
                FailureCategory::Connection
            }
        }
        Phase::StartTls => FailureCategory::Tls,
        Phase::Auth => {
            if error.is_transient()
                || error.is_permanent()
                || error.is_response()
                || error.is_client()
            {
                FailureCategory::Authentication
            } else {
                FailureCategory::Connection
            }
        }
        Phase::Send => {
            if error.is_transient() {
                FailureCategory::TemporaryRefusal
            } else if error.is_permanent() || error.is_client() {
                FailureCategory::PermanentRefusal
            } else {
                FailureCategory::Connection
            }
        }
    }
}

/// Build the message: sender, Reply-To and a stable Message-ID come from the
/// configuration and job, the recipient and text from the payload.
///
/// THREAT: addresses are typed mailboxes and the header encoder escapes the
/// subject, so payload text cannot add headers or recipients.
fn compose(
    settings: &MailTransportConfig,
    mail: &OutgoingMail,
) -> Result<Message, FailureCategory> {
    let mut builder = Message::builder()
        .from(settings.from.clone())
        .reply_to(settings.reply_to.clone())
        .to(mail.recipient.clone())
        .subject(mail.subject.as_str())
        .date_now();
    if let Some(job_id) = mail.job_id {
        builder = builder.message_id(Some(format!("<{job_id}@{}>", settings.from.email.domain())));
    }
    builder
        .header(ContentType::TEXT_PLAIN)
        .body(mail.body.clone())
        .map_err(|_| FailureCategory::PayloadUnreadable)
}

fn tls_parameters(settings: &MailTransportConfig) -> Result<TlsParameters, FailureCategory> {
    TlsParameters::builder(settings.host.clone())
        .build_rustls()
        .map_err(|_| FailureCategory::Tls)
}

async fn session(
    connection: &mut AsyncSmtpConnection,
    settings: &MailTransportConfig,
    message: &Message,
    hello: &ClientId,
) -> Result<(), FailureCategory> {
    if settings.security == SmtpSecurity::Starttls {
        // THREAT: a relay that does not offer STARTTLS must fail the
        // submission; continuing would send credentials and mail in clear.
        if !connection.can_starttls() {
            return Err(FailureCategory::Tls);
        }
        connection
            .starttls(tls_parameters(settings)?, hello)
            .await
            .map_err(|error| categorise(&error, Phase::StartTls))?;
    }
    if let Some(credentials) = &settings.credentials {
        let credentials = Credentials::new(
            credentials.username().to_owned(),
            credentials.password().to_owned(),
        );
        connection
            .auth(&[Mechanism::Plain, Mechanism::Login], &credentials)
            .await
            .map_err(|error| categorise(&error, Phase::Auth))?;
    }
    connection
        .send(message.envelope(), &message.formatted())
        .await
        .map_err(|error| categorise(&error, Phase::Send))?;
    Ok(())
}

async fn exchange(
    settings: &MailTransportConfig,
    message: &Message,
) -> Result<AsyncSmtpConnection, FailureCategory> {
    let hello = ClientId::default();
    let implicit = if settings.security == SmtpSecurity::ImplicitTls {
        Some(tls_parameters(settings)?)
    } else {
        None
    };
    let mut connection = AsyncSmtpConnection::connect_tokio1(
        (settings.host.as_str(), settings.port),
        Some(SUBMISSION_TIMEOUT),
        &hello,
        implicit,
        None,
    )
    .await
    .map_err(|error| categorise(&error, Phase::Connect))?;

    if let Err(error) = session(&mut connection, settings, message, &hello).await {
        connection.abort().await;
        return Err(error);
    }
    Ok(connection)
}

/// Submit one message, succeeding only once the relay positively accepts it.
///
/// The whole exchange is bounded by [`SUBMISSION_TIMEOUT`]. The error is a
/// safe category: no relay response, address or credential is retained.
///
/// # Errors
///
/// Returns the [`FailureCategory`] describing the first failing step.
pub async fn submit(
    settings: &MailTransportConfig,
    mail: &OutgoingMail,
) -> Result<(), FailureCategory> {
    let message = compose(settings, mail)?;
    let deadline = tokio::time::Instant::now() + SUBMISSION_TIMEOUT;
    let mut connection = tokio::time::timeout_at(deadline, exchange(settings, &message))
        .await
        .unwrap_or(Err(FailureCategory::Timeout))?;
    if !matches!(
        tokio::time::timeout_at(deadline, connection.quit()).await,
        Ok(Ok(_))
    ) {
        tracing::debug!("relay closed the session without acknowledging QUIT");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use secrecy::SecretString;
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        net::{TcpListener, TcpStream},
    };

    use super::*;
    use crate::config::RelayCredentials;

    type Log = Arc<Mutex<Vec<String>>>;

    #[derive(Clone)]
    struct Script {
        greeting: &'static str,
        extensions: Vec<&'static str>,
        starttls: &'static str,
        auth: &'static str,
        rcpt: &'static str,
        end_of_data: &'static str,
        hang_after_greeting: bool,
        hang_on_quit: bool,
    }

    impl Default for Script {
        fn default() -> Self {
            Self {
                greeting: "220 fake.example ESMTP",
                extensions: Vec::new(),
                starttls: "220 ready to start TLS",
                auth: "235 authenticated",
                rcpt: "250 recipient ok",
                end_of_data: "250 queued as 42",
                hang_after_greeting: false,
                hang_on_quit: false,
            }
        }
    }

    async fn reply(stream: &mut tokio::net::tcp::OwnedWriteHalf, text: &str) {
        stream
            .write_all(format!("{text}\r\n").as_bytes())
            .await
            .unwrap();
    }

    async fn ehlo(writer: &mut tokio::net::tcp::OwnedWriteHalf, script: &Script) {
        let mut lines = vec!["fake.example"];
        lines.extend(script.extensions.iter().copied());
        let last = lines.len() - 1;
        for (index, text) in lines.iter().enumerate() {
            let separator = if index == last { ' ' } else { '-' };
            reply(writer, &format!("250{separator}{text}")).await;
        }
    }

    async fn read_data(reader: &mut BufReader<tokio::net::tcp::OwnedReadHalf>) -> Option<String> {
        let mut data = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                return None;
            }
            if line == ".\r\n" {
                return Some(data);
            }
            data.push_str(&line);
        }
    }

    async fn serve(stream: TcpStream, script: Script, log: Log) {
        let (reader, mut writer) = stream.into_split();
        let mut reader = BufReader::new(reader);
        reply(&mut writer, script.greeting).await;
        if script.hang_after_greeting {
            std::future::pending::<()>().await;
        }
        let mut line = String::new();
        loop {
            line.clear();
            if reader.read_line(&mut line).await.unwrap_or(0) == 0 {
                break;
            }
            let command = line.trim_end().to_owned();
            log.lock().unwrap().push(command.clone());
            let upper = command.to_ascii_uppercase();
            if upper.starts_with("EHLO") {
                ehlo(&mut writer, &script).await;
            } else if upper.starts_with("STARTTLS") {
                reply(&mut writer, script.starttls).await;
            } else if upper.starts_with("AUTH") {
                reply(&mut writer, script.auth).await;
            } else if upper.starts_with("MAIL FROM") {
                reply(&mut writer, "250 sender ok").await;
            } else if upper.starts_with("RCPT TO") {
                reply(&mut writer, script.rcpt).await;
            } else if upper.starts_with("DATA") {
                reply(&mut writer, "354 end with <CRLF>.<CRLF>").await;
                let Some(data) = read_data(&mut reader).await else {
                    return;
                };
                log.lock().unwrap().push(format!("DATA\n{data}"));
                reply(&mut writer, script.end_of_data).await;
            } else if upper.starts_with("QUIT") {
                if script.hang_on_quit {
                    std::future::pending::<()>().await;
                }
                reply(&mut writer, "221 bye").await;
                break;
            } else {
                reply(&mut writer, "502 not implemented").await;
            }
        }
    }

    async fn fake_relay(script: Script) -> (u16, Log) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let log = Log::default();
        let accept_log = Arc::clone(&log);
        tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                tokio::spawn(serve(stream, script.clone(), Arc::clone(&accept_log)));
            }
        });
        (port, log)
    }

    fn mailbox(address: &str) -> Mailbox {
        Mailbox::new(None, address.parse().unwrap())
    }

    fn settings(port: u16, security: SmtpSecurity, login: bool) -> MailTransportConfig {
        MailTransportConfig {
            host: "127.0.0.1".into(),
            port,
            security,
            credentials: login.then(|| {
                RelayCredentials::new("relay-user".into(), SecretString::from("relay-pass"))
            }),
            from: mailbox("reverie@example.com"),
            reply_to: mailbox("support@example.com"),
        }
    }

    fn mail() -> OutgoingMail {
        OutgoingMail {
            recipient: mailbox("reader@example.org"),
            subject: "Your link".into(),
            body: "Open the link within 15 minutes.".into(),
            job_id: Some(Uuid::from_u128(0x0123_4567_89ab_cdef_0123_4567_89ab_cdef)),
        }
    }

    fn commands(log: &Log) -> Vec<String> {
        log.lock().unwrap().clone()
    }

    fn sent(log: &Log, prefix: &str) -> bool {
        commands(log).iter().any(|line| line.starts_with(prefix))
    }

    #[tokio::test]
    async fn smtp_transport_payload_text_cannot_inject_headers_or_recipients() {
        let (port, log) = fake_relay(Script::default()).await;
        let hostile = OutgoingMail {
            subject: "Hi\r\nBcc: attacker@example.net".into(),
            body: "line\r\n.\r\nRCPT TO:<attacker@example.net>\r\n".into(),
            ..mail()
        };
        let result = submit(&settings(port, SmtpSecurity::Plaintext, false), &hostile).await;
        assert_eq!(result, Ok(()));
        let log = commands(&log);
        assert_eq!(
            log.iter()
                .filter(|line| line.starts_with("RCPT TO"))
                .count(),
            1,
            "{log:?}"
        );
        let data = log.iter().find(|line| line.starts_with("DATA\n")).unwrap();
        assert!(!data.lines().any(|line| line.starts_with("Bcc:")), "{data}");
        assert!(
            !log.iter()
                .any(|line| line.contains("attacker") && line.starts_with("RCPT"))
        );
    }

    #[tokio::test]
    async fn smtp_transport_errors_carry_no_relay_text_or_addresses() {
        let (port, _) = fake_relay(Script {
            rcpt: "550 5.1.1 relay-marker reader@example.org unknown",
            ..Script::default()
        })
        .await;
        let error = submit(&settings(port, SmtpSecurity::Plaintext, true), &mail())
            .await
            .unwrap_err();
        let diagnostic = format!("{error} {error:?} {}", error.as_str());
        for marker in [
            "relay-marker",
            "reader@example.org",
            "relay-pass",
            "relay-user",
        ] {
            assert!(!diagnostic.contains(marker), "{marker} in {diagnostic}");
        }
        assert!(std::error::Error::source(&error).is_none());
    }

    #[tokio::test]
    async fn smtp_transport_starttls_not_offered_fails_without_downgrade() {
        let (port, log) = fake_relay(Script {
            extensions: vec!["AUTH PLAIN"],
            ..Script::default()
        })
        .await;
        let result = submit(&settings(port, SmtpSecurity::Starttls, true), &mail()).await;
        assert_eq!(result, Err(FailureCategory::Tls));
        assert!(
            !sent(&log, "AUTH") && !sent(&log, "MAIL FROM") && !sent(&log, "DATA"),
            "{:?}",
            commands(&log)
        );
    }

    #[tokio::test]
    async fn smtp_transport_starttls_refused_by_relay_fails_without_downgrade() {
        let (port, log) = fake_relay(Script {
            extensions: vec!["STARTTLS", "AUTH PLAIN"],
            starttls: "454 4.7.0 relay-marker TLS not available",
            ..Script::default()
        })
        .await;
        let result = submit(&settings(port, SmtpSecurity::Starttls, true), &mail()).await;
        assert_eq!(result, Err(FailureCategory::Tls));
        assert!(sent(&log, "STARTTLS"));
        assert!(
            !sent(&log, "AUTH") && !sent(&log, "MAIL FROM") && !sent(&log, "DATA"),
            "{:?}",
            commands(&log)
        );
    }

    #[tokio::test]
    async fn smtp_transport_implicit_tls_against_non_tls_peer_is_a_tls_failure() {
        let (port, log) = fake_relay(Script::default()).await;
        let result = submit(&settings(port, SmtpSecurity::ImplicitTls, true), &mail()).await;
        assert_eq!(result, Err(FailureCategory::Tls));
        assert!(
            !sent(&log, "AUTH") && !sent(&log, "MAIL FROM"),
            "{:?}",
            commands(&log)
        );
    }
}
