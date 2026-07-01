---
severity: low
surfaces: [security, frontend, server-operator]
adopted: 2026-06-30
adopted-because: S3 review decision; the register endpoint mints immediately-active accounts with no admin approval, which is unsafe for a multi-user, potentially internet-exposed instance where anyone who can reach it could mint a working account
lift-when-class: feature
lift-when: registration is moderated as a request-access flow (creates disabled-pending accounts an admin approves), with the `/register` route and a gated login entry point re-wired and a disabled-on-register invariant test
---

# Self-registration ships dormant (endpoint off by default, screen unrouted)

The S3 work added self-service registration on both ends, but it is intentionally
left inert:

- Backend: `POST /auth/register` (`backend/src/routes/auth.rs`) is gated on
  `self_registration_enabled`, which defaults to `false`. When enabled it calls
  `user::create_local(..., Role::Adult, ...)`, which sets no `disabled_at`, so a
  registered account is immediately active and can log in with no admin step.
- Frontend: `frontend/src/routes/auth-register.tsx` plus the `register` client
  and `RegisterSchema` exist and are unit-tested, but `main.tsx` registers no
  `/register` route, so the screen is unreachable in the app.

As built, turning the flag on is an unmoderated free-for-all: anyone who can
reach the instance creates a working adult account. On a multi-user instance that
may be reachable from the public internet, that is an open door, and it fits
neither Reverie's threat model nor the norm for self-hosted media servers
(admin-provisioned accounts). Account creation therefore stays admin-only via
`POST /api/v1/users`; the registration path is retained but not exposed.

The code is kept rather than deleted because it is the starting point for a
moderated flow: `POST /auth/register` should create the account **disabled**
(reusing the existing soft-disable column and `set_disabled`), an admin approves
it from the Users page via the existing account-status endpoint, and only then
do the `/register` route and a `self_registration_enabled`-gated login entry
point get wired up. A follow-up issue carries that work and is the lift
condition for this entry.
