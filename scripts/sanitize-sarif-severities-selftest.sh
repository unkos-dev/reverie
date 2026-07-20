#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
sanitizer="${root}/scripts/sanitize-sarif-severities.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

# Shape mirrors where Snyk puts the property: rule metadata under
# tool.driver.rules[].properties, with results left untouched.
cat >"${tmp}/in.sarif" <<'EOF'
{
  "runs": [
    {
      "tool": {
        "driver": {
          "rules": [
            {"id": "no-cvss", "properties": {"security-severity": null}},
            {"id": "license", "properties": {"security-severity": "undefined"}},
            {"id": "string-cvss", "properties": {"security-severity": "7.5"}},
            {"id": "numeric-cvss", "properties": {"security-severity": 9.8}},
            {"id": "no-severity", "properties": {"tags": ["security"]}}
          ]
        }
      },
      "results": [{"ruleId": "no-cvss", "level": "warning"}]
    }
  ]
}
EOF

"$sanitizer" "${tmp}/in.sarif" "${tmp}/out.sarif"

fail=0
expect_severity() {
  local name="$1" want="$2"
  local got
  got=$(jq -c --arg id "$name" \
    '.runs[0].tool.driver.rules[] | select(.id == $id).properties["security-severity"]' \
    "${tmp}/out.sarif")
  if [ "$got" != "$want" ]; then
    echo "FAIL ${name}: expected severity ${want}, got ${got}"
    fail=1
  else
    echo "ok   ${name}"
  fi
}

expect_severity "no-cvss" "null"
expect_severity "license" "null"
expect_severity "string-cvss" '"7.5"'
expect_severity "numeric-cvss" "9.8"

# Dropping means the key is gone, not nulled: a literal null would still
# be rejected by the upload processor.
if [ "$(jq '[.. | objects | select(has("security-severity"))] | length' "${tmp}/out.sarif")" = "2" ]; then
  echo "ok   invalid keys removed outright"
else
  echo "FAIL invalid keys removed outright: a non-numeric security-severity survived"
  fail=1
fi

if [ "$(jq -c '.runs[0].results' "${tmp}/out.sarif")" = '[{"ruleId":"no-cvss","level":"warning"}]' ]; then
  echo "ok   results untouched"
else
  echo "FAIL results untouched"
  fail=1
fi

got=0
"$sanitizer" "${tmp}/in.sarif" >/dev/null 2>&1 || got=$?
if [ "$got" -eq 2 ]; then
  echo "ok   missing output arg rejected"
else
  echo "FAIL missing output arg rejected: expected exit 2, got ${got}"
  fail=1
fi

echo 'not json' >"${tmp}/bad.sarif"
if ! "$sanitizer" "${tmp}/bad.sarif" "${tmp}/bad-out.sarif" >/dev/null 2>&1; then
  echo "ok   malformed input fails loud"
else
  echo "FAIL malformed input fails loud: sanitizer exited 0"
  fail=1
fi

exit "$fail"
