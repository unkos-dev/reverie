---
type: ADR
profile-version: 1
id: "REV-ADR-0064"
title: "Passkeys as the default sign-in, with optional passwords, OIDC and mail"
status: "accepted"
recorded-on: "2026-10-23"
decided-on: "2026-10-19"
decision-makers:
  - "John Unkovich"
---

# Passkeys as the default sign-in, with optional passwords, OIDC and mail

## Context and problem statement

Interactive sign-in uses local passwords and an optional OIDC provider. Passwords can be phished, guessed and reused,
and an enabled password stays a way into the account even after the holder has a stronger credential. A typical
installation is a Docker Compose service reached first over LAN HTTP, often with no domain or certificate, while some
operators expose it to the internet behind a reverse proxy. Browsers expose WebAuthn only in secure contexts, so
passkeys need HTTPS or a browser on the same machine.

Which sign-in method should Reverie lead with, which methods should stay optional, and how should enrolment, recovery
and deployment fit that choice? The decision covers interactive browser sign-in. API and e-reader device tokens, session
lifetimes, password-strength policy and self-registration are outside it.

## Decision drivers

- Give users who can use it a phishing-resistant sign-in.
- Keep a first installation over LAN HTTP, with no domain or certificate, usable from another device.
- Require no external service: an identity provider and a mail relay both stay optional.
- Let an account, or a whole instance, drop passwords once a working alternative exists.
- Keep one account per person, with no separate household account type, and never give an administrator another person's
  credential.
- Give every user an authorised way to replace a lost credential, without a path that quietly turns passwords back on.
- Stop password guessing from blocking other sign-in methods or ending existing sessions.
- Leave protocol and cryptography to maintained libraries rather than Reverie code.

## Considered options

Sign-in methods:

- Passkeys by default, with passwords that can be disabled and optional OIDC
- Passwords and OIDC as co-equal modes, with passkeys as an optional extra
- Passkeys only
- An external identity provider for all interactive sign-in

Transport security:

- TLS at a reverse proxy, with direct LAN HTTP for passwords
- Native TLS termination in Reverie
- HTTPS required for every installation

## Decision outcome

Chosen option: **Passkeys by default, with passwords that can be disabled and optional OIDC**, because it gives
phishing-resistant sign-in wherever the browser and origin allow it, while a minimal installation still works and no
external service becomes mandatory. For transport security, the chosen option is
**TLS at a reverse proxy, with direct LAN HTTP for passwords**, because it keeps the minimal installation and leaves
certificate handling to tools built for it.

Reverie offers passkeys first wherever the browser supports them on a secure origin. A maintained WebAuthn library
performs registration and authentication; Reverie owns the product policy around it. Local passwords stay available, and
they can be disabled for one account or for the whole instance. The instance setting overrides account preferences. An
account disables passwords only after an alternative sign-in method is verified, and the instance setting has its own
readiness check. The server enforces a disabled password, and neither HTTP access nor recovery re-enables it. OIDC
remains an optional, operator-configured method, and accounts are still matched on issuer and subject.

Installation begins with onboarding the first administrator. Other people join through an invitation bound to their
account, and they enrol a credential they control. On an origin without passkey support, the invitation enrols a
password instead. Credential replacement is one flow with several authorised proofs: an existing credential, a
previously verified email address, an administrator for an ordinary user, or host access for an administrator. A
surviving session alone does not authorise binding a new credential. Mail stays optional: when configured, it carries
invitations, verified-email recovery and security notices; without it, administrator and host recovery still work.

Each sign-in method draws on its own admission allowance, so password failures neither exhaust passkey, OIDC or recovery
admission nor end existing sessions. Password admission blocks temporarily and never disables a password permanently.

A reverse proxy terminates TLS and Reverie serves HTTP behind it on a restricted listener. Reverie also accepts direct
LAN HTTP, where only passwords work. Adding HTTPS later enables passkey enrolment for existing accounts without
recreating them.

Passwords and OIDC as co-equal modes leave the password as every account's main point of attack. Passkeys only would
lock out LAN HTTP installations and anyone without a supported browser or passkey device. An external identity provider
for everything makes a separate service mandatory. Native TLS termination would put certificate issuance and renewal
inside Reverie and still need a certificate that browsers trust. Requiring HTTPS everywhere would rule out the minimal
installation.

### Consequences

- Positive: users who enrol a passkey get sign-in that is bound to Reverie's domain and resists phishing, and the server
  holds no shared secret for it.
- Positive: a Compose installation with no domain or certificate still works over LAN HTTP, and no identity provider or
  mail relay is required.
- Positive: operators can remove passwords from an account or instance once alternatives are in place, which removes the
  password attack surface there.
- Negative: the instance now owns three interactive credential types plus enrolment and recovery flows, all
  security-critical.
- Negative: plain-HTTP installations cannot use passkeys and stay password-only. HTTP also leaves credentials and
  session traffic without encryption, so it is unsuitable for public exposure.
- Negative: passkeys are bound to the relying-party domain. A hostname change can strand them, and those users then need
  recovery or re-enrolment.
- Negative: a sustained targeted attack can repeatedly obstruct sign-in for password-only users, and an enabled password
  stays a compromise path even for accounts that also hold a passkey.
- Negative: temporary password blocks do not implement NIST SP 800-63B-4's rule of disabling a credential after 100
  consecutive failures.
- Negative: passkey phishing resistance does not extend to password fallback, enrolment or recovery.
- Negative: without mail or a verified address, an ordinary user depends on an administrator to recover.
- Negative: browser-trusted HTTPS needs a reverse proxy and a certificate, which may mean a DNS challenge or a private
  certificate authority on a private network.
- Neutral: a shared device's PIN does not separate the people who use it. Account switching on shared devices is
  validated rather than assumed.

## Pros and cons of the options

### Passkeys by default, with passwords that can be disabled and optional OIDC

- Positive: phishing-resistant sign-in wherever the origin allows it, with passwords still available elsewhere.
- Positive: passwords can be removed once alternatives exist, rather than staying a permanent fallback.
- Negative: readiness checks, credential management and recovery must cover three credential types.

### Passwords and OIDC as co-equal modes, with passkeys as an optional extra

- Positive: the smallest change from the current sign-in model.
- Negative: passwords can never be removed, so every account keeps its weakest path.
- Negative: an optional extra method leaves most users on passwords.

### Passkeys only

- Positive: no password to phish, guess or block.
- Negative: an installation needs browser-trusted HTTPS before anyone can sign in, ruling out LAN HTTP setup.
- Negative: anyone without a supported browser or passkey device cannot use Reverie.

### An external identity provider for all interactive sign-in

- Positive: passkeys, multi-factor authentication and recovery come from a dedicated identity service.
- Negative: every installation must run or rent an identity provider, which the unified identity model rejected as a
  requirement.

### TLS at a reverse proxy, with direct LAN HTTP for passwords

- Positive: mature proxies handle certificates and renewal, and existing proxy users keep their setup.
- Negative: browser-trusted HTTPS needs a second component and some certificate knowledge.
- Negative: the proxy-to-Reverie hop needs restricted access and trusted forwarded-header handling.

### Native TLS termination in Reverie

- Positive: a single container could serve HTTPS.
- Negative: Reverie would own certificate issuance, renewal and key storage.
- Negative: a private installation still needs a certificate that browsers trust before passkeys work.

### HTTPS required for every installation

- Positive: credential traffic is always encrypted.
- Negative: users without a domain or certificate cannot finish the first installation.

## More information

This decision refines
[Unified identity with pluggable authentication providers](0029-unified-identity-with-pluggable-authentication-providers.md),
which deferred passkeys and treated passwords and OIDC as co-equal modes.
[Password admission with committed failure accounting](0059-password-admission-with-committed-failure-accounting.md)
records password admission and separate source allowances.
[Administrators issue account-bound credential links](0061-administrators-issue-account-bound-credential-links.md)
records invitations and administrator-assisted recovery, and
[Optional SMTP delivery through an encrypted durable mail queue](0063-optional-smtp-delivery-through-an-encrypted-durable-mail-queue.md)
records optional mail.

The choice of WebAuthn library, the relying-party identifier and hostname-change policy, the sign-in interaction, and
the readiness checks for disabling passwords are left to later decisions.

[WebAuthn Level 3](https://www.w3.org/TR/webauthn-3/) exposes its API only in secure contexts and scopes a credential to
a relying-party identifier that is the origin's domain or a parent domain of it.
[NIST SP 800-63B-4](https://pages.nist.gov/800-63-4/sp800-63b.html) section 3.2.2 limits consecutive failures on one
credential to 100 by disabling it.

Reconsider this decision if Reverie adds native TLS termination, or if browsers make WebAuthn available outside secure
contexts.
