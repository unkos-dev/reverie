---
severity: low
surfaces: [developer, ci]
adopted: 2026-10-22
adopted-because: the pinned workflow linter rejects GitHub's supported Ubuntu 26.04 ARM runner label
lift-when-class: dep-unblocks
lift-when: the pinned workflow linter recognises the Ubuntu 26.04 ARM runner label and workflow lint passes without the additional label
---

# Ubuntu 26.04 ARM runner label exception

The Docker build and publish workflows use GitHub's hosted `ubuntu-26.04-arm` runner. `actionlint` 1.7.12 rejects that
label because its built-in runner catalogue predates support. `.github/actionlint.yaml` accepts that exact label through
`self-hosted-runner.labels`, although the runner is hosted by GitHub. Other runner labels and workflow checks retain
their existing validation.

GitHub lists the label in its [available runner images](https://github.com/actions/runner-images#available-images).
`actionlint` tracks native support in [its upstream pull request](https://github.com/rhysd/actionlint/pull/743).

When the pinned `actionlint` release recognises the label, delete `.github/actionlint.yaml` and run
`just infra::actionlint`. Remove this entry and its README link once that check passes without the additional label.
