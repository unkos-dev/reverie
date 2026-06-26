---
severity: low
surfaces: [developer, ci]
adopted: 2026-06-26
adopted-because: discovered during PR #511 review; the gate passed source carrying real plan-ref labels
lift-when-class: internal-refactor
lift-when: no-plan-refs.sh recurses into backend/src and frontend/src subdirectories AND matches capitalized Decision/Invariant/Phase labels, with a test asserting both
---

# `no-plan-refs.sh` misses labels in subdirectories and capitalized forms

`scripts/no-plan-refs.sh` is meant to reject plan-artifact labels (`(S2)`,
`decision N`, `invariant N`, `Phase N`) in `backend/src` and `frontend/src`
source. Two defects let real labels through:

1. **Scope glob does not recurse.** `is_gated()` matches `backend/src/*.rs`,
   but a bash `case` glob `*` does not span `/`, so only files directly under
   `backend/src/` are gated. Source under `backend/src/routes/`,
   `backend/src/models/`, etc. is never checked.
2. **Pattern is case-sensitive lowercase.** The regex matches `decision N` and
   `invariant N` but not the capitalized `Decision N` / `Invariant N` forms that
   appear in practice (and `Phase N` only via the capitalized branch).

Because of (1) and (2) together, labels such as `Decision 6`, `Invariant 1`, and
a line-wrapped `Phase 2` shipped in `routes/auth.rs` and `models/user.rs` and
were only caught by manual review. The labels were stripped in that PR; the gate
itself still has the coverage gaps.

Lift by making the scope check recurse (e.g. iterate the candidate list rather
than glob-match a fixed depth, or use `**`) and case-insensitive matching, with
a test that feeds a nested file containing `Decision 1` and asserts a non-zero
exit.
