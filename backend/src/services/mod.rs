//! Business-logic services orchestrating the application's domain work.
//!
//! Each submodule owns one major pipeline phase or capability — the
//! ingestion watcher (`ingestion`), third-party metadata fetch +
//! cache + dry-run (`enrichment`), EPUB layered reader/writer
//! (`epub`), cover acquisition + resize (`covers`), metadata
//! drafting + sanitisation (`metadata`), and canonical-metadata flush
//! back into source files (`writeback`).

/// Cover-image extraction, resize, and on-disk cache lookup.
pub mod covers;
pub mod enrichment;
/// EPUB layered reader/writer: zip-level entry IO, container, OPF,
/// cover injection, XHTML extraction, repack, and validation/repair.
pub mod epub;
pub mod files;
pub mod ingestion;
/// Metadata extraction, drafting, sanitisation, ISBN normalisation,
/// and the value-vs-canonical inversion helpers.
pub mod metadata;
/// Periodic reaper deleting expired rows from `tower_sessions.session`.
pub mod session_sweep;
/// Persisted settings load, save, validation, and LISTEN/NOTIFY reload.
pub mod settings;
pub mod writeback;

pub(crate) const BULK_RETRY_LIMIT: i64 = 500;

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub(crate) struct BulkRetryResult {
    #[schema(minimum = 0, maximum = 500)]
    pub queued: i64,
    #[schema(minimum = 0)]
    pub remaining: i64,
}

#[derive(Debug)]
pub(crate) struct BulkRetryAudit {
    user_id: uuid::Uuid,
    request_id: uuid::Uuid,
    source_ip: Option<std::net::IpAddr>,
    user_agent: Option<String>,
}

impl BulkRetryAudit {
    pub(crate) fn new(
        user_id: uuid::Uuid,
        headers: &axum::http::HeaderMap,
        peer: Option<std::net::IpAddr>,
        trusted_header: Option<&str>,
    ) -> Self {
        // THREAT: Untrusted user-agent text is bounded and stripped of control characters before logging.
        let user_agent = headers.get(axum::http::header::USER_AGENT).map(|value| {
            String::from_utf8_lossy(value.as_bytes())
                .chars()
                .filter(|c| !c.is_control())
                .take(256)
                .collect()
        });
        Self {
            user_id,
            request_id: uuid::Uuid::new_v4(),
            source_ip: crate::auth::rate_limit::client_ip(headers, peer, trusted_header),
            user_agent,
        }
    }

    pub(crate) fn committed(
        &self,
        target: &'static str,
        group: &'static str,
        source: Option<&str>,
        queued: u64,
        remaining: i64,
    ) {
        tracing::info!(
            action = "bulk_retry",
            outcome = "committed",
            user_id = %self.user_id,
            request_id = %self.request_id,
            source_ip = self.source_ip.map(|ip| ip.to_string()),
            user_agent = self.user_agent.as_deref(),
            target,
            group,
            source,
            queued,
            remaining,
            "bulk retry committed"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bulk_retry_audit_sanitises_metadata_and_excludes_credentials() {
        let user_id = uuid::Uuid::new_v4();
        let mut headers = axum::http::HeaderMap::new();
        headers.insert(
            "authorization",
            "Bearer private-credential".parse().unwrap(),
        );
        headers.insert("cookie", "id=private-session".parse().unwrap());
        headers.insert(
            "user-agent",
            format!("reader\t{}", "x".repeat(300)).parse().unwrap(),
        );
        headers.insert("x-forwarded-for", "192.0.2.2".parse().unwrap());
        let peer = "192.0.2.1".parse().unwrap();
        let audit = BulkRetryAudit::new(user_id, &headers, Some(peer), None);
        assert_eq!(audit.user_agent.as_ref().unwrap().len(), 256);
        assert!(!audit.user_agent.as_ref().unwrap().contains('\t'));
        assert_eq!(audit.source_ip, Some(peer));
        let logs = crate::test_support::CapturedLogs::default();
        tracing::subscriber::with_default(logs.subscriber(), || {
            audit.committed(
                "enrichment_failures",
                "timeout",
                Some("openlibrary"),
                500,
                1,
            );
        });
        let text = logs.text();
        for field in [
            "action=\"bulk_retry\"",
            "outcome=\"committed\"",
            "target=\"enrichment_failures\"",
            "group=\"timeout\"",
            "source=\"openlibrary\"",
            "queued=500",
            "remaining=1",
            &format!("user_id={user_id}"),
            &format!("request_id={}", audit.request_id),
            "source_ip=\"192.0.2.1\"",
            "user_agent=\"reader",
        ] {
            assert!(text.contains(field), "missing {field}: {text}");
        }
        for secret in [
            "private-credential",
            "private-session",
            "authorization",
            "cookie",
            "192.0.2.2",
        ] {
            assert!(!text.contains(secret));
        }
        let forwarded = BulkRetryAudit::new(user_id, &headers, Some(peer), Some("x-forwarded-for"));
        assert_eq!(forwarded.source_ip, Some("192.0.2.2".parse().unwrap()));
        assert_ne!(forwarded.request_id, audit.request_id);
    }
}
