#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
checker="${root}/scripts/no-agent-attribution.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

git -C "$tmp" init -q -b main
git -C "$tmp" config user.name "Attribution Selftest"
git -C "$tmp" config user.email "selftest@example.invalid"
git -C "$tmp" config commit.gpgsign false

fail=0
expect_file() {
  local name="$1" want="$2" body="$3" got=0 file="${tmp}/message file"
  printf '%s\n' "$body" >"$file"
  "$checker" message-file "$file" >/dev/null 2>&1 || got=$?
  if [ "$got" -ne "$want" ]; then
    echo "FAIL ${name}: expected exit ${want}, got ${got}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

expect_file "plain commit passes" 0 "chore: normal message"
expect_file "body label passes" 0 $'chore: document protocol\n\nAgent: is an application field, not attribution.\n\nNormal footer text.'
expect_file "human co-author passes" 0 $'chore: collaborate\n\nCo-authored-by: Jane Doe <jane@example.com>'
expect_file "human sharing a tool first name passes" 0 $'chore: collaborate\n\nCo-authored-by: Claude Smith <claude@example.com>'
expect_file "DCO trailer passes" 0 $'chore: signed\n\nSigned-off-by: Jane Doe <jane@example.com>'
expect_file "generated trailer rejected" 1 $'chore: generated\n\nGenerated-by: tooling'
expect_file "session trailer rejected" 1 $'chore: linked\n\nSession-Link: https://example.invalid/session'
expect_file "Codex co-author rejected" 1 $'chore: attributed\n\nCo-authored-by: Codex <codex@example.invalid>'
expect_file "Claude co-author case variant rejected" 1 $'chore: attributed\n\nCO-AUTHORED-BY: claude <noreply@anthropic.com>'
expect_file "model co-author rejected" 1 $'chore: attributed\n\nCo-authored-by: GPT-5 <gpt@example.invalid>'
for identity in ChatGPT 'GitHub Copilot' Gemini Cursor Windsurf; do
  expect_file "${identity} co-author rejected" 1 "$(printf 'chore: attributed\n\nCo-authored-by: %s <tool@example.invalid>' "$identity")"
done
got=0
"$checker" message-file "${tmp}/missing message" >/dev/null 2>&1 || got=$?
[ "$got" -eq 2 ] || { echo "FAIL missing message file: expected exit 2, got ${got}"; fail=1; }

printf x >"${tmp}/tracked"
git -C "$tmp" add tracked
git -C "$tmp" commit -q -m "chore: base"
git -C "$tmp" tag base
printf y >>"${tmp}/tracked"
git -C "$tmp" commit -qam $'chore: human\n\nCo-authored-by: Jane Doe <jane@example.com>'
git -C "$tmp" tag clean

got=0
(cd "$tmp" && "$checker" range base clean) >/dev/null 2>&1 || got=$?
[ "$got" -eq 0 ] || { echo "FAIL clean range: expected exit 0, got ${got}"; fail=1; }

printf z >>"${tmp}/tracked"
git -C "$tmp" commit -qam $'chore: generated\n\nGenerated-With: tool'
git -C "$tmp" tag attributed
got=0
(cd "$tmp" && "$checker" range base attributed) >/dev/null 2>&1 || got=$?
[ "$got" -eq 1 ] || { echo "FAIL attributed range: expected exit 1, got ${got}"; fail=1; }

git -C "$tmp" checkout -q -b side clean
printf s >"${tmp}/side"
git -C "$tmp" add side
git -C "$tmp" commit -q -m "chore: side"
git -C "$tmp" checkout -q -b merge-main clean
git -C "$tmp" merge -q --no-ff --no-edit side
git -C "$tmp" tag merged
got=0
(cd "$tmp" && "$checker" range clean merged) >/dev/null 2>&1 || got=$?
[ "$got" -eq 0 ] || { echo "FAIL merge commit exempt: expected exit 0, got ${got}"; fail=1; }

got=0
(cd "$tmp" && "$checker" range base no-such-ref) >/dev/null 2>&1 || got=$?
[ "$got" -eq 2 ] || { echo "FAIL invalid range: expected exit 2, got ${got}"; fail=1; }

got=0
(cd "$tmp" && "$checker" range clean clean) >/dev/null 2>&1 || got=$?
[ "$got" -eq 0 ] || { echo "FAIL empty range: expected exit 0, got ${got}"; fail=1; }

exit "$fail"
