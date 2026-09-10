---
severity: medium
surfaces: [developer, ci]
adopted: 2026-09-10
adopted-because: pnpm 12 generates inconsistent Vite specifiers during Renovate lockfile updates
lift-when-class: dep-unblocks
lift-when: a stable pnpm 12 or later release passes both targeted update reproductions and clean frozen installs with the existing Vite catalogue and override
---

# pnpm updates stay on v11

The workspace uses pnpm 11.26.0. Renovate limits pnpm and its Docker image to v11 because pnpm 12.3.2 through 12.4.0
write Vite importer specifiers that their frozen installer rejects. Earlier v12 releases reject the update command's
script-disabling flag.

The failing command is:

```sh
pnpm update --no-save <package>@<version> --lockfile-only --recursive --ignore-scripts --ignore-pnpmfile
```

Lift the restriction when a stable release passes this command followed by a clean
`pnpm install --frozen-lockfile --ignore-scripts` for both `impeccable@4.1.0` and `lucide-react@1.42.0`, starting
separately from the parent of PR #1069. Retain the Vite catalogue and override. Verify the current workspace with
`just preflight`, then remove the Renovate restriction and this entry together.
