---
severity: medium
surfaces: [developer, ci]
adopted: 2026-10-26
adopted-because: slow Azure Ubuntu package downloads repeatedly exhausted the frontend job's twenty-minute limit
lift-when-class: dep-unblocks
lift-when: the hosted runner image fixes slow mirror failover and frontend tests and accessibility checks pass with its unmodified mirror list
---

# Frontend CI Ubuntu mirror selection

The frontend test and accessibility jobs remove `azure.archive.ubuntu.com` from `/etc/apt/apt-mirrors.txt` before
Playwright installs system dependencies. They first require the existing HTTPS Ubuntu archive entry, so a changed runner
layout fails visibly instead of leaving the intended download source unavailable. Ubuntu's archive and security mirrors
retain their configured priorities, package signatures and repository keys.

Azure package downloads at 52 to 68 kB/s consumed more than fifteen minutes of a frontend test job and exhausted its
twenty-minute limit across repeated runs. GitHub acknowledges the slow-transfer behaviour in its
[runner-image issue](https://github.com/actions/runner-images/issues/14823).

When the hosted image fixes slow mirror failover, remove both mirror-selection steps and verify frontend tests and
accessibility provisioning with the image's unmodified mirror list. Delete this entry and its README link once that
verification passes. Browser dependencies, test assertions and job timeouts retain their existing configuration.
