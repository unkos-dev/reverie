# Screen design specs

Each file here specifies one approved screen: what it is for, where it lives, how it is laid out, every state with its
trigger and exact copy, the keyboard and accessibility expectations, and an acceptance checklist.

A change to a screen is checked against its spec. The change is done when the rendered page matches the spec, in both
themes, and not only when the tests pass. A change to a screen's behaviour, copy or layout updates its spec in the same
change. Where no spec covers a surface, the change stays mechanical and the gap is raised before anything is designed.

These are the approved specifications each screen is accepted against, written without a change history. Parts of a
screen that are approved but not yet implemented are marked as target design inside the spec that owns them.

The wider design rules live in [Design Philosophy](../philosophy.md), [Visual Identity](../visual-identity.md) and
[Color Tokens](../color-tokens.md).

The table lists every page the application routes to. A screen without a linked spec is shipped without an approved
design.

| Screen | Route | Reached from |
| ------ | ----- | ------------ |
| Library | `/library` | Left rail; `/` redirects here |
| Book | `/b/{id}` | Library, command palette, ingestion |
| Series | `/series/{id}` | Series label on a book page |
| Shelves | `/shelves` | Left rail |
| Shelf | `/shelves/{id}` | Left rail, shelves list |
| API tokens | `/tokens` | User menu |
| Change password | `/account/password` | User menu, local-password accounts |
| First-run setup | `/setup` | Redirect before the first administrator |
| [Settings](./settings.md) | `/admin/settings/{area}` | User menu, administrators |
| [Admin ingestion](./admin-ingestion.md) | `/admin/dashboard` | Left rail, administrators |
| [Users](./users.md) | `/admin/users` | Left rail, administrators |
| [Login](./login.md) | `/login` | Redirect when signed out |
| [Set password](./set-password.md) | `/set-password`, `/reset-password` | Single-use link |
| [Forgot password](./forgot-password.md) | `/forgot-password` | Login |
| [Email verification](./email-verification.md) | `/account/email`, `/verify-email` | User menu; verification link |
