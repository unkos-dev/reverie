#!/usr/bin/env bash
set -euo pipefail

HOOK="$(cd "$(dirname "$0")/.." && pwd)/redact-secrets-output.sh"
PASS=0
FAIL=0

run_test() {
  local name="$1" input_stdout="$2" input_stderr="$3" expected_pattern="$4" expect_redaction="$5" check_field="$6"
  local secret_absent="${7:-}"

  local payload
  payload=$(jq -n \
    --arg stdout "$input_stdout" \
    --arg stderr "$input_stderr" \
    '{
      tool_name: "Bash",
      tool_input: {command: "test-command"},
      tool_response: {stdout: $stdout, stderr: $stderr, interrupted: false, isImage: false},
      tool_use_id: "test-123"
    }')

  local output
  output=$(printf '%s' "$payload" | bash "$HOOK" 2>/dev/null) || true

  if [[ "$expect_redaction" == "yes" ]]; then
    if [[ -z "$output" ]]; then
      printf 'FAIL: %s — expected redaction but got pass-through\n' "$name"
      FAIL=$(( FAIL + 1 ))
      return
    fi
    local actual
    actual=$(printf '%s' "$output" | jq -r ".hookSpecificOutput.updatedToolOutput.${check_field}" 2>/dev/null || true)
    if printf '%s' "$actual" | rg -q "$expected_pattern" 2>/dev/null; then
      # Also verify the original secret value is absent from redacted output
      if [[ -n "$secret_absent" ]] && printf '%s' "$actual" | rg -qF "$secret_absent" 2>/dev/null; then
        printf 'FAIL: %s — secret value "%s" still present after redaction\n' "$name" "$secret_absent"
        FAIL=$(( FAIL + 1 ))
      else
        printf 'PASS: %s\n' "$name"
        PASS=$(( PASS + 1 ))
      fi
    else
      printf 'FAIL: %s — expected pattern "%s" in %s, got: %s\n' "$name" "$expected_pattern" "$check_field" "$actual"
      FAIL=$(( FAIL + 1 ))
    fi
  else
    if [[ -z "$output" ]]; then
      printf 'PASS: %s (pass-through)\n' "$name"
      PASS=$(( PASS + 1 ))
    else
      printf 'FAIL: %s — expected pass-through but got output: %s\n' "$name" "$output"
      FAIL=$(( FAIL + 1 ))
    fi
  fi
}

printf '=== PostToolUse Secret Redaction Hook Tests ===\n\n'

# Pattern 1a: KEY=VALUE (env var form)
run_test \
  "Pattern 1a: KEY=VALUE env var" \
  "QBITTORRENT_PASSWORD=testvalue123" \
  "" \
  'QBITTORRENT_PASSWORD=\[REDACTED\]' \
  "yes" \
  "stdout" \
  "testvalue123"

# Pattern 1b: KEY: "VALUE" (JSON/YAML colon form)
run_test \
  "Pattern 1b: JSON colon form" \
  '"DB_SECRET": "hunter2secretvalue"' \
  "" \
  'DB_SECRET=\[REDACTED\]' \
  "yes" \
  "stdout" \
  "hunter2secretvalue"

# Pattern 2: URL-embedded credentials
run_test \
  "Pattern 2: URL credentials" \
  'postgres://admin:hunter2secret@db.example.com:5432/reverie' \
  "" \
  '\[REDACTED:url-creds\]' \
  "yes" \
  "stdout" \
  "hunter2secret"

# Pattern 3: Bearer token
run_test \
  "Pattern 3: Bearer token" \
  'Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.abc123456' \
  "" \
  'Bearer \[REDACTED\]' \
  "yes" \
  "stdout" \
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9"

# Pattern 4a: GitHub classic PAT
run_test \
  "Pattern 4a: GitHub classic PAT" \
  'ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZab' \
  "" \
  '\[REDACTED:github-pat\]' \
  "yes" \
  "stdout" \
  "ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ"

# Pattern 4b: GitHub fine-grained PAT
run_test \
  "Pattern 4b: GitHub fine-grained PAT" \
  'github_pat_11AABBBCCC_xxxxxxxxxxxxxxxxxxxx' \
  "" \
  '\[REDACTED:github-pat\]' \
  "yes" \
  "stdout" \
  "github_pat_11AABBBCCC"

# Pattern 5a: Context7 key
run_test \
  "Pattern 5a: Context7 API key" \
  'ctx7sk-abcdefghijklmnopqrstuvwx' \
  "" \
  '\[REDACTED:api-key\]' \
  "yes" \
  "stdout" \
  "ctx7sk-abcdefghijklmnop"

# Pattern 5b: Cloudflare token
run_test \
  "Pattern 5b: Cloudflare token" \
  'cfat_sL3727gAbCdEfGhIjKlMnOpQr' \
  "" \
  '\[REDACTED:api-key\]' \
  "yes" \
  "stdout" \
  "cfat_sL3727gAbCdEfGh"

# Pattern 5c: CodeRabbit key (cr- + 50+ hex)
run_test \
  "Pattern 5c: CodeRabbit API key" \
  'cr-aabbccddeeff00112233445566778899aabbccddeeff00112233' \
  "" \
  '\[REDACTED:api-key\]' \
  "yes" \
  "stdout" \
  "cr-aabbccddeeff0011223344"

# False positive: git commit SHA
run_test \
  "False positive: git SHA" \
  'commit abc123def456789abcdef0123456789abcdef01' \
  "" \
  "" \
  "no" \
  "stdout"

# False positive: short hex container ID
run_test \
  "False positive: short container ID" \
  'container_id=a1b2c3d4e5f6' \
  "" \
  "" \
  "no" \
  "stdout"

# False positive: UUID
run_test \
  "False positive: UUID" \
  'uuid: 550e8400-e29b-41d4-a716-446655440000' \
  "" \
  "" \
  "no" \
  "stdout"

# False positive: empty value after =
run_test \
  "False positive: empty TOKEN=" \
  'SOME_TOKEN=' \
  "" \
  "" \
  "no" \
  "stdout"

# False positive: short URL password (under 8 chars, not matched)
run_test \
  "False positive: short URL password" \
  'postgres://user:abc@localhost:5432/db' \
  "" \
  "" \
  "no" \
  "stdout"

# Partial redaction: mixed secret and non-secret lines
run_test \
  "Partial redaction: mixed content" \
  $'STATUS: running\nDB_PASSWORD=supersecret123\nPORT: 5432' \
  "" \
  'DB_PASSWORD=\[REDACTED\]' \
  "yes" \
  "stdout" \
  "supersecret123"

# Stderr redaction
run_test \
  "Stderr redaction: Pattern 1a" \
  "" \
  "POSTGRES_PASSWORD=hunter2secret" \
  'POSTGRES_PASSWORD=\[REDACTED\]' \
  "yes" \
  "stderr" \
  "hunter2secret"

# Empty stdout + empty stderr = pass-through
run_test \
  "Pass-through: empty output" \
  "" \
  "" \
  "" \
  "no" \
  "stdout"

# Verify original fields preserved in updatedToolOutput
printf '\n--- Field preservation test ---\n'
PRESERVE_PAYLOAD=$(jq -n '{
  tool_name: "Bash",
  tool_input: {command: "test"},
  tool_response: {stdout: "MY_SECRET_TOKEN=leaked123value", stderr: "", interrupted: true, isImage: false, backgroundTaskId: "bg-42"},
  tool_use_id: "test-preserve"
}')
PRESERVE_OUTPUT=$(printf '%s' "$PRESERVE_PAYLOAD" | bash "$HOOK" 2>/dev/null) || true
PRESERVED_INTERRUPTED=$(printf '%s' "$PRESERVE_OUTPUT" | jq -r '.hookSpecificOutput.updatedToolOutput.interrupted' 2>/dev/null || true)
PRESERVED_BG_ID=$(printf '%s' "$PRESERVE_OUTPUT" | jq -r '.hookSpecificOutput.updatedToolOutput.backgroundTaskId' 2>/dev/null || true)
if [[ "$PRESERVED_INTERRUPTED" == "true" && "$PRESERVED_BG_ID" == "bg-42" ]]; then
  printf 'PASS: field preservation (interrupted=%s, backgroundTaskId=%s)\n' "$PRESERVED_INTERRUPTED" "$PRESERVED_BG_ID"
  PASS=$(( PASS + 1 ))
else
  printf 'FAIL: field preservation — interrupted=%s (expected true), backgroundTaskId=%s (expected bg-42)\n' "$PRESERVED_INTERRUPTED" "$PRESERVED_BG_ID"
  FAIL=$(( FAIL + 1 ))
fi

# Fail-closed test: malformed JSON input should produce blanked output, not pass-through
printf '\n--- Fail-closed tests ---\n'
MALFORMED_OUTPUT=$(printf 'this is not json at all' | bash "$HOOK" 2>/dev/null) || true
if [[ -n "$MALFORMED_OUTPUT" ]] && printf '%s' "$MALFORMED_OUTPUT" | rg -q 'OUTPUT SUPPRESSED' 2>/dev/null; then
  printf 'PASS: fail-closed on malformed JSON input\n'
  PASS=$(( PASS + 1 ))
else
  printf 'FAIL: fail-closed — malformed JSON should produce blanked output, got: %s\n' "${MALFORMED_OUTPUT:-(empty/pass-through)}"
  FAIL=$(( FAIL + 1 ))
fi

printf '\n=== Results: %d passed, %d failed ===\n' "$PASS" "$FAIL"
[[ $FAIL -eq 0 ]] && exit 0 || exit 1
