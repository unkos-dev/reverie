#!/usr/bin/env bash
set -euo pipefail

root="$(git rev-parse --show-toplevel)"
filter="${root}/scripts/filter-sarif-unfixable-os.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "${tmp}"' EXIT

# One rule per case the predicate has to separate. The three "kept" rules
# are the anti-masking cases: a distro package that HAS a fix, an
# application-layer package with no fix, and a rule whose remediation text
# the filter cannot read at all.
cat >"${tmp}/in.sarif" <<'EOF'
{
  "runs": [
    {
      "tool": {"driver": {"rules": [
        {"id": "SNYK-DEBIAN13-CURL-1",
         "help": {"text": "## Remediation\nThere is no fixed version for `Debian:13` `curl`."}},
        {"id": "SNYK-DEBIAN13-PERL-2",
         "help": {"markdown": "## Remediation\nThere is no fixed version for `Debian:13` `perl`."}},
        {"id": "SNYK-DEBIAN14-GLIBC-3",
         "help": {"text": "There is no fixed version for `Debian:14` `glibc`."}},
        {"id": "SNYK-DEBIAN13-OPENSSL-4",
         "help": {"text": "## Remediation\nUpgrade `Debian:13` `openssl` to version 3.5.0-1 or higher."}},
        {"id": "SNYK-JS-LODASH-5",
         "help": {"text": "## Remediation\nThere is no fixed version for `lodash`."}},
        {"id": "SNYK-DEBIAN13-MYSTERY-6", "help": {}},
        {"id": "SNYK-DEBIAN14-ZLIB-7",
         "help": {"text": "There is no fixed version for `Debian:13` `zlib`.\nUpgrade `Debian:14` `zlib` to version 1.3.1-1 or higher."}}
      ]}},
      "results": [
        {"ruleId": "SNYK-DEBIAN13-CURL-1"},
        {"ruleId": "SNYK-DEBIAN13-CURL-1"},
        {"ruleId": "SNYK-DEBIAN13-PERL-2"},
        {"ruleId": "SNYK-DEBIAN14-GLIBC-3"},
        {"ruleId": "SNYK-DEBIAN13-OPENSSL-4"},
        {"ruleId": "SNYK-JS-LODASH-5"},
        {"ruleId": "SNYK-DEBIAN13-MYSTERY-6"},
        {"ruleId": "SNYK-DEBIAN14-ZLIB-7"}
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
kept() {
  jq --arg id "$1" '[.runs[0].results[] | select(.ruleId == $id)] | length' "${tmp}/out.sarif"
}

summary="$("$filter" "${tmp}/in.sarif" "${tmp}/out.sarif")"

check "unfixable distro finding withheld (help.text)" "0" "$(kept SNYK-DEBIAN13-CURL-1)"
check "unfixable distro finding withheld (help.markdown)" "0" "$(kept SNYK-DEBIAN13-PERL-2)"
check "namespace is not pinned to one codename" "0" "$(kept SNYK-DEBIAN14-GLIBC-3)"

check "distro finding WITH a fix is ingested" "1" "$(kept SNYK-DEBIAN13-OPENSSL-4)"
check "application-layer finding with no fix is ingested" "1" "$(kept SNYK-JS-LODASH-5)"
check "unclassifiable finding is ingested (fails closed)" "1" "$(kept SNYK-DEBIAN13-MYSTERY-6)"

# The marker is tied to the release in the rule's own ID. An advisory that
# mentions an older release having no fix while offering an upgrade for
# the scanned one is fixable, and withholding it would be the exact
# failure this filter exists to avoid.
check "no-fix text for another release does not withhold a fixable finding" "1" \
  "$(kept SNYK-DEBIAN14-ZLIB-7)"

check "rule metadata untouched" "7" \
  "$(jq '.runs[0].tool.driver.rules | length' "${tmp}/out.sarif")"

if grep -q 'Scanned 8, ingested 4, withheld 4' <<<"$summary" \
  && grep -q 'curl. | 2' <<<"$summary"; then
  echo "ok   summary reports counts and packages"
else
  echo "FAIL summary reports counts and packages"
  echo "${summary}"
  fail=1
fi

# A scan with nothing to withhold must still produce valid output.
echo '{"runs":[{"tool":{"driver":{"rules":[]}},"results":[]}]}' >"${tmp}/empty.sarif"
"$filter" "${tmp}/empty.sarif" "${tmp}/empty-out.sarif" >/dev/null
check "empty scan passes through" "0" \
  "$(jq '[.runs[]?.results[]?] | length' "${tmp}/empty-out.sarif")"

got=0
"$filter" "${tmp}/in.sarif" >/dev/null 2>&1 || got=$?
check "missing output arg rejected" "2" "$got"

echo 'not json' >"${tmp}/bad.sarif"
if ! "$filter" "${tmp}/bad.sarif" "${tmp}/bad-out.sarif" >/dev/null 2>&1; then
  echo "ok   malformed input fails loud"
else
  echo "FAIL malformed input fails loud: filter exited 0"
  fail=1
fi

exit "$fail"
