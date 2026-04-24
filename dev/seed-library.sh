#!/usr/bin/env bash
#
# Seed a dev library with a curated set of Standard Ebooks public-domain EPUBs.
#
# Target directory: $REVERIE_LIBRARY_ROOT, or backend/tests/fixtures/library/
# relative to the repo root if unset. The fallback directory is gitignored;
# running this script never writes into tracked paths.
#
# Curation (8 titles) deliberately covers design edge cases:
#   - short title:         Austen — Emma
#   - long title:          Stevenson — The Strange Case of Dr Jekyll and Mr Hyde
#   - series:              Conan Doyle — three Sherlock Holmes collections
#   - long author:         Dostoevsky — Crime and Punishment (translated)
#   - translated work:     Tolstoy — Anna Karenina (translated)
#   - rich subject data:   Darwin — The Voyage of the Beagle
#
# Downloads are pinned by URL + SHA-256. A checksum mismatch means
# Standard Ebooks re-issued the title; the correct fix is to update URL
# and SHA-256 together in the same commit — this script never auto-
# overwrites. Idempotent: a second run skips files that already exist and
# whose checksums match.
#
# Standard Ebooks requires `?source=download` on the .epub URL; without it
# their server returns a meta-refresh HTML confirmation page. Do not drop
# this query string.
#
# Usage:
#   ./dev/seed-library.sh
#   REVERIE_LIBRARY_ROOT=/path/to/library ./dev/seed-library.sh

set -euo pipefail

if [[ -z "${REVERIE_LIBRARY_ROOT:-}" ]]; then
  repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  TARGET="${repo_root}/backend/tests/fixtures/library"
else
  TARGET="${REVERIE_LIBRARY_ROOT}"
fi

mkdir -p "${TARGET}"

# Portable sha256: Linux/CI has sha256sum; macOS has shasum.
if command -v sha256sum >/dev/null 2>&1; then
  sha256_of() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  sha256_of() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  echo "error: neither sha256sum nor shasum found on PATH" >&2
  exit 2
fi

# Manifest: "<sha256>  <filename>  <url>" (sha first for easy grep/diff).
# Regenerated 2026-04-24. Update all three fields together if a title is
# re-issued upstream.
MANIFEST=(
  "3ee2544f0fb2c683c645d9e963a598e3f76b3e1e25a02b6ace9a2c9c0c54e942  jane-austen_emma.epub  https://standardebooks.org/ebooks/jane-austen/emma/downloads/jane-austen_emma.epub"
  "4c2b854bf19df70550b4514aa7a63496b76bbbd61fefb08c22a77ddcb691f151  robert-louis-stevenson_the-strange-case-of-dr-jekyll-and-mr-hyde.epub  https://standardebooks.org/ebooks/robert-louis-stevenson/the-strange-case-of-dr-jekyll-and-mr-hyde/downloads/robert-louis-stevenson_the-strange-case-of-dr-jekyll-and-mr-hyde.epub"
  "157731f1eb1f3842d93e5136cdc87d6c00e5358e82c0fa8b7a245df5f5b337ba  arthur-conan-doyle_the-adventures-of-sherlock-holmes.epub  https://standardebooks.org/ebooks/arthur-conan-doyle/the-adventures-of-sherlock-holmes/downloads/arthur-conan-doyle_the-adventures-of-sherlock-holmes.epub"
  "33168e4a0e7a2cd026de78bbb4a3772624d7e1621128e1c4aefc73e3942153f1  arthur-conan-doyle_the-memoirs-of-sherlock-holmes.epub  https://standardebooks.org/ebooks/arthur-conan-doyle/the-memoirs-of-sherlock-holmes/downloads/arthur-conan-doyle_the-memoirs-of-sherlock-holmes.epub"
  "cf1ac9782d5f0c2ce8c455dafe0f2ed1940700734781639c3c1fccdcd68eddcd  arthur-conan-doyle_the-return-of-sherlock-holmes.epub  https://standardebooks.org/ebooks/arthur-conan-doyle/the-return-of-sherlock-holmes/downloads/arthur-conan-doyle_the-return-of-sherlock-holmes.epub"
  "5a0b32f34aa0a387c38509709cde5057e9de039b9a0529a8a25240e014af39e6  fyodor-dostoevsky_crime-and-punishment_constance-garnett.epub  https://standardebooks.org/ebooks/fyodor-dostoevsky/crime-and-punishment/constance-garnett/downloads/fyodor-dostoevsky_crime-and-punishment_constance-garnett.epub"
  "7cbdd42dc030378a5e3ac3d8878019ca8e0cd13321f8ef2d245811f86efd4577  leo-tolstoy_anna-karenina_constance-garnett.epub  https://standardebooks.org/ebooks/leo-tolstoy/anna-karenina/constance-garnett/downloads/leo-tolstoy_anna-karenina_constance-garnett.epub"
  "5a64709e70f3986449fbb6c40e6505ec75dbf8fcb946a2101c81ea2353fa9722  charles-darwin_the-voyage-of-the-beagle.epub  https://standardebooks.org/ebooks/charles-darwin/the-voyage-of-the-beagle/downloads/charles-darwin_the-voyage-of-the-beagle.epub"
)

fetched=0
skipped=0

for entry in "${MANIFEST[@]}"; do
  expected_sha="${entry%%  *}"
  rest="${entry#*  }"
  filename="${rest%%  *}"
  url="${rest#*  }"
  dest="${TARGET}/${filename}"

  if [[ -f "${dest}" ]]; then
    actual_sha=$(sha256_of "${dest}")
    if [[ "${actual_sha}" == "${expected_sha}" ]]; then
      skipped=$((skipped + 1))
      continue
    fi
    echo "error: ${filename} exists but sha256 mismatch" >&2
    echo "  expected: ${expected_sha}" >&2
    echo "  actual:   ${actual_sha}" >&2
    echo "  refusing to overwrite — delete the file manually or update the manifest." >&2
    exit 3
  fi

  echo "fetching ${filename} ..."
  tmp="${dest}.part"
  curl --fail --silent --show-error --location --retry 3 --retry-delay 2 \
    --max-time 180 -o "${tmp}" "${url}?source=download"

  actual_sha=$(sha256_of "${tmp}")
  if [[ "${actual_sha}" != "${expected_sha}" ]]; then
    rm -f "${tmp}"
    echo "error: sha256 mismatch for ${filename}" >&2
    echo "  expected: ${expected_sha}" >&2
    echo "  actual:   ${actual_sha}" >&2
    echo "  a Standard Ebooks re-issue likely shipped; update URL + SHA together." >&2
    exit 4
  fi

  mv "${tmp}" "${dest}"
  fetched=$((fetched + 1))
done

echo "done: fetched=${fetched} skipped=${skipped} target=${TARGET}"
