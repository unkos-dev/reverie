#!/usr/bin/env bash
# PostToolUse hook: scan Bash tool output for secret patterns and redact
# before transcript ingestion. Defence-in-depth for Hard Rule 7.
#
# Input:  JSON on stdin (Claude Code PostToolUse payload)
# Output: exit 0 with hookSpecificOutput JSON = redact; exit 0 silent = pass-through
#         If final JSON assembly fails, emits fail-safe blanked output (fail-closed).
#
# Log:    ~/.claude/hooks/secret-redaction.log contains the original stdout, stderr,
#         command string, and tool_use_id for each redaction event.
#         NEVER cat/bat/Read this file into chat. Use wc -l for event counts.

set -u
umask 077

INPUT="$(cat)"

STDOUT="$(printf '%s' "$INPUT" | jq -r '.tool_response.stdout // empty' 2>/dev/null || true)"
STDERR="$(printf '%s' "$INPUT" | jq -r '.tool_response.stderr // empty' 2>/dev/null || true)"

[[ -z "$STDOUT" && -z "$STDERR" ]] && exit 0

CMD="$(printf '%s' "$INPUT" | jq -r '.tool_input.command // "unknown"' 2>/dev/null || true)"
TOOL_USE_ID="$(printf '%s' "$INPUT" | jq -r '.tool_use_id // "unknown"' 2>/dev/null || true)"

# Character class [^] \t"',}]+ uses POSIX ERE rule: ] immediately after [^ is a
# literal ] (not a class closer). This stops matching at ] so that replacement text
# like [REDACTED] does not get consumed on re-processing. Do NOT move ] away from
# first position in the class.
apply_redactions() {
  /bin/sed -E \
    -e 's/([A-Z_]*(PASSWORD|SECRET|TOKEN|API_KEY|PRIVATE_KEY|PASSPHRASE))=[^] \t"'"'"',}]+/\1=[REDACTED]/gI' \
    -e 's/([A-Z_]*(PASSWORD|SECRET|TOKEN|API_KEY|PRIVATE_KEY|PASSPHRASE))"?:[ \t]*"?[^]"'"'"',}]+/\1=[REDACTED]/gI' \
    -e 's/Bearer [A-Za-z0-9._-]{20,}/Bearer [REDACTED]/gI' \
    -e 's/gh[pousr]_[A-Za-z0-9]{20,}/[REDACTED:github-pat]/g' \
    -e 's/ctx7sk-[A-Za-z0-9_-]{16,}/[REDACTED:api-key]/g' \
    -e 's/cfat_[A-Za-z0-9_-]{16,}/[REDACTED:api-key]/g' \
    -e 's/cr-[a-f0-9]{50,}/[REDACTED:api-key]/g'
}

REDACTED_STDOUT="$(printf '%s' "$STDOUT" | apply_redactions)"
REDACTED_STDERR="$(printf '%s' "$STDERR" | apply_redactions)"

if [[ "$REDACTED_STDOUT" == "$STDOUT" && "$REDACTED_STDERR" == "$STDERR" ]]; then
  exit 0
fi

LOG_FILE="${HOME}/.claude/hooks/secret-redaction.log"
mkdir -p "$(dirname "$LOG_FILE")"

# Truncate if over 1MB.
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
  jq -n '{hookSpecificOutput: {hookEventName: "PostToolUse", additionalContext: "CRITICAL: redaction output assembly failed — stdout/stderr blanked as fail-safe.", updatedToolOutput: {stdout: "[REDACTION ERROR — OUTPUT SUPPRESSED]", stderr: "", interrupted: false, isImage: false}}}'
  exit 0
}
printf '%s\n' "$HOOK_OUTPUT"
