#!/usr/bin/env bash
# PostToolUse hook: scan Bash tool output for secret patterns and redact
# before transcript ingestion. Defence-in-depth for Hard Rule 7.
#
# THREAT: Secret values in Bash tool stdout/stderr enter model context and can
# be re-emitted into subsequent tool calls (commits, PR bodies, API calls,
# env exports) before the operator notices. Redacting at the PostToolUse
# boundary prevents ingestion. Mirrors homelab UNK-282.
#
# Input:  JSON on stdin (Claude Code PostToolUse payload)
# Output: exit 0 with hookSpecificOutput JSON = redact; exit 0 silent = pass-through
#         Any extraction or assembly failure emits blanked output (fail-closed).
#
# Log:    ~/.claude/hooks/secret-redaction.log contains the ORIGINAL (unredacted)
#         stdout, stderr, command string, and tool_use_id for each redaction event.
#         NEVER cat/bat/Read this file into chat.
#         Use rg -c '^=== END ===$' for event counts.

# Intentionally omit -e/-o pipefail: hook must not abort on subcommand error
# (would block the PostToolUse pipeline). set -u for undefined-var guard only.
set -u
# Log file contains plaintext secrets — restrict to owner-only.
umask 077

# Hard-coded fail-safe JSON with zero tool dependencies. Used when jq is
# missing/broken so the catch path never calls the tool that just failed.
emit_blanked() {
  local msg="${1:-redaction failure}"
  printf '{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"CRITICAL: %s — stdout/stderr blanked as fail-safe.","updatedToolOutput":{"stdout":"[REDACTION ERROR — OUTPUT SUPPRESSED]","stderr":"","interrupted":false,"isImage":false}}}\n' "$msg"
}

INPUT="$(cat)"

# Fail closed on extraction: if jq can't parse the payload, blank everything.
STDOUT="$(printf '%s' "$INPUT" | jq -r '.tool_response.stdout // empty' 2>/dev/null)" || {
  emit_blanked "jq failed extracting stdout"
  exit 0
}
STDERR="$(printf '%s' "$INPUT" | jq -r '.tool_response.stderr // empty' 2>/dev/null)" || {
  emit_blanked "jq failed extracting stderr"
  exit 0
}

[[ -z "$STDOUT" && -z "$STDERR" ]] && exit 0

CMD="$(printf '%s' "$INPUT" | jq -r '.tool_input.command // "unknown"' 2>/dev/null)" || CMD="[extraction-failed]"
TOOL_USE_ID="$(printf '%s' "$INPUT" | jq -r '.tool_use_id // "unknown"' 2>/dev/null)" || TOOL_USE_ID="[extraction-failed]"

# Pattern 1 (KEY=VALUE) uses a NEGATED class [^] \t"',}]+ excluding ], space,
#   tab, quotes, comma, } — the POSIX rule "] immediately after [^ is literal"
#   keeps ] inside the class; do NOT move it from first position.
# Pattern 2 (KEY:"val", JSON/YAML) uses a POSITIVE "secret-shaped" class:
#   [A-Za-z0-9._/+=~-]{8,} — a base64/token charset, no whitespace, min 8 chars.
#   THREAT/tradeoff (UNK-326): redacting only secret-shaped values stops the
#   PostToolUse scanner from blanking non-secret values that sit under keys
#   literally named *TOKEN/SECRET/etc in legitimate JSON (e.g. `gh api`
#   responses) — booleans, numbers, short enum strings, prose. The {8,} floor
#   mirrors the URL-cred pattern below (line for ://creds requires {8,}); the
#   no-whitespace class mirrors Pattern 1. Residual: a 1–7 char JSON secret
#   value passes through, consistent with the hook's existing 8-char floor. A
#   value indistinguishable from a real secret (≥8 no-space token charset) is
#   still redacted — fail-safe is preserved for the cases that matter.
#   The / inside the class is escaped (\/) because it is the sed s/// delimiter.
apply_redactions() {
  /bin/sed -E \
    -e 's/([A-Z_]*(PASSWORD|SECRET|TOKEN|API_KEY|PRIVATE_KEY|PASSPHRASE))=[^] \t"'"'"',}]+/\1=[REDACTED]/gI' \
    -e 's/([A-Z_]*(PASSWORD|SECRET|TOKEN|API_KEY|PRIVATE_KEY|PASSPHRASE))"?:[ \t]*"?[A-Za-z0-9._\/+=~-]{8,}/\1=[REDACTED]/gI' \
    -e 's|://[^/:@]+:[^@]{8,}@|://[REDACTED:url-creds]@|g' \
    -e 's/Bearer [A-Za-z0-9._-]{20,}/Bearer [REDACTED]/gI' \
    -e 's/gh[pousr]_[A-Za-z0-9]{20,}/[REDACTED:github-pat]/g' \
    -e 's/github_pat_[A-Za-z0-9_]{20,}/[REDACTED:github-pat]/g' \
    -e 's/ctx7sk-[A-Za-z0-9_-]{16,}/[REDACTED:api-key]/g' \
    -e 's/cfat_[A-Za-z0-9_-]{16,}/[REDACTED:api-key]/g' \
    -e 's/cr-[a-f0-9]{50,}/[REDACTED:api-key]/g'
}

REDACTED_STDOUT="$(printf '%s' "$STDOUT" | apply_redactions)" || {
  emit_blanked "sed failed redacting stdout"
  exit 0
}
REDACTED_STDERR="$(printf '%s' "$STDERR" | apply_redactions)" || {
  emit_blanked "sed failed redacting stderr"
  exit 0
}

if [[ "$REDACTED_STDOUT" == "$STDOUT" && "$REDACTED_STDERR" == "$STDERR" ]]; then
  exit 0
fi

LOG_FILE="${HOME}/.claude/hooks/secret-redaction.log"
mkdir -p "$(dirname "$LOG_FILE")"

# Truncate-to-empty at 1MB. Loses history — acceptable since log is
# forensic-only; unlimited growth worse on long-running instances.
if [[ -f "$LOG_FILE" ]] && (( $(stat -c%s "$LOG_FILE" 2>/dev/null || echo 0) > 1048576 )); then
  : > "$LOG_FILE"
fi

{
  printf '%s\n' "=== $(date -u '+%Y-%m-%dT%H:%M:%SZ') ==="
  printf '%s\n' "tool_use_id: $TOOL_USE_ID"
  printf '%s\n' "command: $CMD"
  printf '%s\n' "--- original stdout ---"
  printf '%s\n' "$STDOUT"
  printf '%s\n' "--- original stderr ---"
  printf '%s\n' "$STDERR"
  printf '%s\n' "=== END ==="
  printf '\n'
} >> "$LOG_FILE"

# Fail-closed: if jq can't assemble the redacted output, emit blanked output
# rather than allowing the original (secret-containing) output through.
HOOK_OUTPUT="$(printf '%s' "$INPUT" | jq \
  --arg stdout "$REDACTED_STDOUT" \
  --arg stderr "$REDACTED_STDERR" \
  '{
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      additionalContext: "WARNING: Hard Rule 7 defence — secret patterns detected and redacted from tool output. Original logged to ~/.claude/hooks/secret-redaction.log for operator review.",
      updatedToolOutput: (.tool_response + {stdout: $stdout, stderr: $stderr})
    }
  }' 2>/dev/null)" || {
  printf '%s\n' "redact-secrets-output: jq failed assembling redacted output for tool_use_id=$TOOL_USE_ID — emitting blanked output" >&2
  emit_blanked "redaction output assembly failed"
  exit 0
}
printf '%s\n' "$HOOK_OUTPUT"
