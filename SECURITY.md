# Security

Reverie is open-source software intended for self-hosted deployment. This
file covers how to report a vulnerability in Reverie itself - not in a
specific deployment of it.

## Supported versions

We're pre-v1.0, so only the most recent 0.x release gets security patches.
Older builds are unsupported; if you're running one, expect to upgrade
before we'll have anything useful to say.

## Reporting a vulnerability

Use GitHub's private advisory path:
<https://github.com/unkos-dev/reverie/security/advisories/new>

Don't open a public issue, post in discussions, or disclose anywhere else
until a fix is out. A public heads-up before a patch exists gives operators
no time to react.

Please include:

- What the issue is
- Steps to reproduce it against a local build
- Affected version or commit SHA
- Your read on impact and who's at risk

We'll acknowledge the report within 72 hours and give you an initial
assessment within seven days. A fix typically ships within 90 days; easier
issues are much faster, and we'll keep you posted either way.

If a vulnerability warrants a CVE, GitHub Security Advisories can issue one
through the automatic-CVE flow at publication time.

## What's in scope

The source code, default configuration, database migrations, and
dependency choices Reverie ships with. If the project's defaults are less
secure than they should be, that's a bug we want to hear about.

## What's not in scope

- A specific operator's deployment. If you've found an issue with someone's
  instance, report it to them.
- Third-party services Reverie integrates with. Report those upstream. The
  exception: if Reverie's integration code is what enables the exploit,
  the integration code itself is in scope.
- Issues that require physical access or the operator's admin credentials
  to trigger.

## Threat model

Reverie is designed around a multi-user instance exposed to the public
internet. Single-user home-LAN-behind-a-VPN deployments still benefit from
the defaults, but they aren't the primary target — decisions about defaults
lean toward the exposed case.

Security-relevant rules for this repo are tracked in
[`.claude/security/`](.claude/security/), which imports Project CodeGuard's
core categories. That's the nearest thing to a canonical reference for
what we consider reasonable care here.

## Safe harbor

Good-faith security research, within the scope above, is welcome. If
you're testing a deployment you operate, not touching data you don't own,
and going through the reporting process described here, we won't pursue
legal action.

## Credit

Unless you'd rather remain anonymous, your name (or handle) goes in the
release notes and the associated GitHub Security Advisory. Tell us at
report time if you want your name on it.
