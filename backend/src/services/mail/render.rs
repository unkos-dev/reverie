//! Subjects and plain-text bodies for authentication mail.
//!
//! Every link is `{origin}/route#{token}` with the origin taken from the
//! configured public URL, and every date is written out in full in UTC.

use chrono::{DateTime, Utc};

use crate::models::mail_job::MailPurpose;

/// A rendered message, before it is sealed with its recipient.
#[derive(Debug, PartialEq, Eq)]
pub struct Message {
    /// Subject line.
    pub subject: String,
    /// Plain-text body.
    pub body: String,
}

/// Instance-wide values every template reads.
#[derive(Debug, Clone)]
pub struct Context {
    /// Public origin without a trailing slash.
    pub origin: String,
    /// The configured support address, also the Reply-To.
    pub support: String,
}

/// Which administrator-link template and route apply, decided by whether the
/// account has a password when the mail is queued.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    /// The account has no password yet: `/set-password`.
    Set,
    /// The account has a password: `/reset-password`.
    Reset,
}

/// An account event that sends a security notice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A password was replaced.
    PasswordChanged,
    /// A first password was set.
    PasswordSet,
    /// The address was verified.
    AddressVerified,
    /// A verified address was changed; sent to the old address.
    AddressChanged,
    /// A verified address was removed; sent to the old address.
    AddressRemoved,
}

impl Notice {
    /// The mail purpose that carries this notice.
    #[must_use]
    pub const fn purpose(self) -> MailPurpose {
        match self {
            Self::PasswordChanged => MailPurpose::NoticePasswordChanged,
            Self::PasswordSet => MailPurpose::NoticePasswordSet,
            Self::AddressVerified => MailPurpose::NoticeAddressVerified,
            Self::AddressChanged => MailPurpose::NoticeAddressChanged,
            Self::AddressRemoved => MailPurpose::NoticeAddressRemoved,
        }
    }
}

/// Write an instant as `{weekday} {day} {month} at {hour}:{minute} {am|pm} UTC`,
/// for example `Saturday 25 October at 2:00 pm UTC`.
#[must_use]
pub fn format_date(at: DateTime<Utc>) -> String {
    at.format("%A %-d %B at %-I:%M %P UTC").to_string()
}

/// The email an administrator chooses to send for a credential link.
#[must_use]
pub fn admin_link(
    context: &Context,
    kind: LinkKind,
    email: &str,
    token: &str,
    expires_at: DateTime<Utc>,
) -> Message {
    let Context { origin, support } = context;
    let expiry = format_date(expires_at);
    match kind {
        LinkKind::Set => Message {
            subject: "Set your Reverie password".to_owned(),
            body: format!(
                "An administrator invited you to set a password for the Reverie account {email} at {origin}.\n\
                 \n\
                 To choose your password, open this link:\n\
                 {origin}/set-password#{token}\n\
                 \n\
                 The link works once and expires on {expiry}.\n\
                 \n\
                 If you weren't expecting this, ignore this email or write to {support}."
            ),
        },
        LinkKind::Reset => Message {
            subject: "Reset your Reverie password".to_owned(),
            body: format!(
                "An administrator started a password reset for the Reverie account {email} at {origin}.\n\
                 \n\
                 To choose a new password, open this link:\n\
                 {origin}/reset-password#{token}\n\
                 \n\
                 The link works once and expires on {expiry}. Your current password keeps working until the link is used. Using it signs the account out everywhere.\n\
                 \n\
                 If you weren't expecting this, write to {support}."
            ),
        },
    }
}

/// The email a password recovery request sends to an eligible account.
#[must_use]
pub fn recovery(context: &Context, email: &str, token: &str, expires_at: DateTime<Utc>) -> Message {
    let Context { origin, support } = context;
    let expiry = format_date(expires_at);
    Message {
        subject: "Reset your Reverie password".to_owned(),
        body: format!(
            "Someone asked to reset the password for the Reverie account {email} at {origin}.\n\
             \n\
             To choose a new password, open this link:\n\
             {origin}/reset-password#{token}\n\
             \n\
             The link works once and expires on {expiry}. Using it signs the account out everywhere.\n\
             \n\
             If you didn't ask for this, ignore this email. This request did not change your password. To report a problem, reply to this email or write to {support}."
        ),
    }
}

/// The email that carries a mailbox verification challenge.
#[must_use]
pub fn verification(
    context: &Context,
    email: &str,
    token: &str,
    expires_at: DateTime<Utc>,
) -> Message {
    let Context { origin, support } = context;
    let expiry = format_date(expires_at);
    Message {
        subject: "Verify your email address for Reverie".to_owned(),
        body: format!(
            "Someone asked to verify {email} for a Reverie account at {origin}.\n\
             \n\
             To confirm it, sign in to that account and open this link:\n\
             {origin}/verify-email#{token}\n\
             \n\
             The link works once and expires on {expiry}.\n\
             \n\
             If you didn't ask for this, ignore this email. To report a problem, reply to this email or write to {support}."
        ),
    }
}

/// A security notice. `email` is the account address the event concerns; the
/// two address notices do not quote it because they go to the former address.
#[must_use]
pub fn notice(context: &Context, event: Notice, email: &str, at: DateTime<Utc>) -> Message {
    let Context { origin, support } = context;
    let time = format_date(at);
    match event {
        Notice::PasswordChanged => Message {
            subject: "Your Reverie password was changed".to_owned(),
            body: format!(
                "The password for the Reverie account {email} at {origin} was changed on {time}. All sessions were signed out. If you didn't make this change, write to {support} straight away."
            ),
        },
        Notice::PasswordSet => Message {
            subject: "Your Reverie password is set".to_owned(),
            body: format!(
                "A password was set for the Reverie account {email} at {origin} on {time}. If you didn't do this, write to {support} straight away."
            ),
        },
        Notice::AddressVerified => Message {
            subject: "Your email address is verified".to_owned(),
            body: format!(
                "{email} is now verified for your Reverie account at {origin}. If you didn't do this, write to {support}."
            ),
        },
        Notice::AddressChanged => Message {
            subject: "Your Reverie email address was changed".to_owned(),
            body: format!(
                "An administrator changed the email address for your Reverie account at {origin} on {time}. This address is no longer associated with your account. Previously queued security notices may still arrive. If you didn't expect this, write to {support}."
            ),
        },
        Notice::AddressRemoved => Message {
            subject: "Your email address was removed from Reverie".to_owned(),
            body: format!(
                "An administrator removed the email address from your Reverie account at {origin} on {time}. This address is no longer associated with your account. Previously queued security notices may still arrive. If you didn't expect this, write to {support}."
            ),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    fn context() -> Context {
        Context {
            origin: "https://reverie.example.org".to_owned(),
            support: "help@example.org".to_owned(),
        }
    }

    fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
            .unwrap()
    }

    const TOKEN: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ";

    #[test]
    fn smtp_admin_delivery_templates_render_exactly() {
        let set = admin_link(
            &context(),
            LinkKind::Set,
            "reader@example.org",
            TOKEN,
            at(25, 14, 0),
        );
        assert_eq!(set.subject, "Set your Reverie password");
        assert_eq!(
            set.body,
            format!(
                "An administrator invited you to set a password for the Reverie account reader@example.org at https://reverie.example.org.\n\nTo choose your password, open this link:\nhttps://reverie.example.org/set-password#{TOKEN}\n\nThe link works once and expires on Sunday 25 October at 2:00 pm UTC.\n\nIf you weren't expecting this, ignore this email or write to help@example.org."
            )
        );
        let reset = admin_link(
            &context(),
            LinkKind::Reset,
            "reader@example.org",
            TOKEN,
            at(25, 14, 0),
        );
        assert_eq!(reset.subject, "Reset your Reverie password");
        assert_eq!(
            reset.body,
            format!(
                "An administrator started a password reset for the Reverie account reader@example.org at https://reverie.example.org.\n\nTo choose a new password, open this link:\nhttps://reverie.example.org/reset-password#{TOKEN}\n\nThe link works once and expires on Sunday 25 October at 2:00 pm UTC. Your current password keeps working until the link is used. Using it signs the account out everywhere.\n\nIf you weren't expecting this, write to help@example.org."
            )
        );
    }

    #[test]
    fn smtp_recovery_template_renders_exactly() {
        let message = recovery(&context(), "reader@example.org", TOKEN, at(25, 14, 15));
        assert_eq!(message.subject, "Reset your Reverie password");
        assert_eq!(
            message.body,
            format!(
                "Someone asked to reset the password for the Reverie account reader@example.org at https://reverie.example.org.\n\nTo choose a new password, open this link:\nhttps://reverie.example.org/reset-password#{TOKEN}\n\nThe link works once and expires on Sunday 25 October at 2:15 pm UTC. Using it signs the account out everywhere.\n\nIf you didn't ask for this, ignore this email. This request did not change your password. To report a problem, reply to this email or write to help@example.org."
            )
        );
    }

    #[test]
    fn smtp_verification_template_renders_exactly() {
        let message = verification(&context(), "reader@example.org", TOKEN, at(26, 9, 0));
        assert_eq!(message.subject, "Verify your email address for Reverie");
        assert_eq!(
            message.body,
            format!(
                "Someone asked to verify reader@example.org for a Reverie account at https://reverie.example.org.\n\nTo confirm it, sign in to that account and open this link:\nhttps://reverie.example.org/verify-email#{TOKEN}\n\nThe link works once and expires on Monday 26 October at 9:00 am UTC.\n\nIf you didn't ask for this, ignore this email. To report a problem, reply to this email or write to help@example.org."
            )
        );
    }
}
