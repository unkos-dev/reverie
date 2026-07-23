#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
filter="${root}/scripts/filter-sarif-test-rules.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

cat >"${tmp}/allow.txt" <<'EOF'
# comment line is ignored

javascript/NoHardcodedPasswords/test
EOF

# The third result is the one that matters: an ordinary rule ID reported
# in a test file. It must survive, because that is the case where a real
# defect happens to live in test code.
cat >"${tmp}/in.sarif" <<'EOF'
{
  "runs": [
    {
      "tool": {"driver": {"rules": [
        {"id": "javascript/NoHardcodedPasswords/test"},
        {"id": "javascript/Xss"},
        {"id": "javascript/InsufficientPostmessageValidation"}
      ]}},
      "results": [
        {"ruleId": "javascript/NoHardcodedPasswords/test",
         "locations": [{"physicalLocation": {"artifactLocation": {"uri": "src/api/auth.test.ts"}}}]},
        {"ruleId": "javascript/NoHardcodedPasswords/test",
         "locations": [{"physicalLocation": {"artifactLocation": {"uri": "src/api/auth.test.ts"}}}]},
        {"ruleId": "javascript/Xss",
         "locations": [{"physicalLocation": {"artifactLocation": {"uri": "src/api/auth.test.ts"}}}]},
        {"ruleId": "javascript/InsufficientPostmessageValidation",
         "locations": [{"physicalLocation": {"artifactLocation": {"uri": "src/lib/Theme.tsx"}}}]}
      ]
    }
  ]
}
EOF

fail=0
check() {
  local name="$1" want="$2" got="$3"
  if [ "$got" = "$want" ]; then
    echo "ok   ${name}"
  else
    echo "FAIL ${name}: expected ${want}, got ${got}"
    fail=1
  fi
}

summary="$("$filter" "${tmp}/in.sarif" "${tmp}/out.sarif" "${tmp}/allow.txt")"

check "allowlisted test-variant results dropped" "0" \
  "$(jq '[.runs[0].results[] | select(.ruleId == "javascript/NoHardcodedPasswords/test")] | length' "${tmp}/out.sarif")"

check "ordinary rule in a test file survives" "1" \
  "$(jq '[.runs[0].results[] | select(.ruleId == "javascript/Xss")] | length' "${tmp}/out.sarif")"

check "ordinary rule in production file survives" "1" \
  "$(jq '[.runs[0].results[] | select(.ruleId == "javascript/InsufficientPostmessageValidation")] | length' "${tmp}/out.sarif")"

check "rule metadata untouched" "3" \
  "$(jq '.runs[0].tool.driver.rules | length' "${tmp}/out.sarif")"

if grep -q 'Analysed 4, ingested 2, withheld 2' <<<"$summary" \
  && grep -q 'src/api/auth.test.ts' <<<"$summary"; then
  echo "ok   summary reports counts and files"
else
  echo "FAIL summary reports counts and files"
  echo "${summary}"
  fail=1
fi

# An unregistered `/test` rule must stop the run rather than be dropped
# silently or pass through unremarked.
cat >"${tmp}/unlisted.sarif" <<'EOF'
{"runs": [{"tool": {"driver": {"rules": [{"id": "javascript/NewRule/test"}]}},
           "results": [{"ruleId": "javascript/NewRule/test"}]}]}
EOF
got=0
err="$("$filter" "${tmp}/unlisted.sarif" "${tmp}/unlisted-out.sarif" "${tmp}/allow.txt" 2>&1 >/dev/null)" || got=$?
if [ "$got" -eq 1 ] && grep -q 'javascript/NewRule/test' <<<"$err"; then
  echo "ok   unregistered test-variant rule fails loud"
else
  echo "FAIL unregistered test-variant rule fails loud: exit ${got}"
  fail=1
fi

got=0
"$filter" "${tmp}/in.sarif" "${tmp}/out.sarif" >/dev/null 2>&1 || got=$?
check "missing allowlist arg rejected" "2" "$got"

got=0
"$filter" "${tmp}/in.sarif" "${tmp}/out.sarif" "${tmp}/absent.txt" >/dev/null 2>&1 || got=$?
check "unreadable allowlist rejected" "2" "$got"

echo 'not json' >"${tmp}/bad.sarif"
if ! "$filter" "${tmp}/bad.sarif" "${tmp}/bad-out.sarif" "${tmp}/allow.txt" >/dev/null 2>&1; then
  echo "ok   malformed input fails loud"
else
  echo "FAIL malformed input fails loud: filter exited 0"
  fail=1
fi

exit "$fail"
