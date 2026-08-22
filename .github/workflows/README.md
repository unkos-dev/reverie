# CI composition

This directory holds one workflow per concern plus `ci.yml`, which calls them.
The design is meant to be copied, so this document separates the portable
skeleton from the values that are specific to this repository.

Workflow YAML here stays comment-light. A comment in a workflow file explains
something a reader of that file cannot see; everything structural lives here.

## The composition pattern

`ci.yml` is a thin caller. It owns the triggers, top-level permissions,
concurrency, and one job per concern:

```yaml
jobs:
  lint:
    uses: ./.github/workflows/lint.yml
```

Each concern file is `workflow_call`-only and filters itself internally, using
its own `changes` job over the shared path filters.

The alternative, one large workflow with every job inline, fails in two ways
that matter. A red check names the workflow rather than the concern, so the
first thing a reader does is open a thousand-line file and search it. And the
enforcement surface collapses to a single aggregate context, which hides which
concerns are actually required.

### Why check runs are named the way they are

A job in a called workflow reports as `<caller job> / <called job>`. The `lint`
caller job invoking a `prose` job inside `lint.yml` produces the check run
`lint / prose`. Required contexts are therefore namespaced by construction, and
the branch ruleset lists them per job rather than as one aggregate.

Job ids are the display names. Concern files set no `name:` key on any job, so
the id is what appears in the context string. Short kebab ids keep the contexts
readable.

## Two invariants that will block every pull request if violated

Both failure modes end the same way: a required context never reports, and
pull requests sit pending forever with nothing to click.

**A caller job never gains an `if:` condition.** A skipped caller reports a
check run named `lint`, but the ruleset requires `lint / prose`. That context
never reports. Filtering belongs inside the called workflow, where a skipped
job reports success and satisfies its own context.

**No workflow carrying required contexts gets a top-level `paths:` trigger.**
A workflow that never triggers reports nothing at all, which is not the same
as reporting success. Path filtering happens in a `changes` job, never in the
trigger.

The two rules are the same rule seen from different levels: something that does
not run must still report, and only a job that GitHub started can report.

## The changes job and path filters

Every concern file starts with a `changes` job running `dorny/paths-filter`
against `.github/path-filters.yml`, and exposes only the filters that concern
needs as outputs.

The filter file is shared with `scripts/preflight-scope.sh`, which decides
which lanes a local gate runs. One file, so a local gate and CI cannot disagree
about what a change touches.

Two details are easy to get wrong. The `merge_group` base defaults to `HEAD~1`,
which on a synthetic merge ref sees only the topmost commit, so the filter pins
`base` and `ref` explicitly. And a filter that feeds a guard a file list passes
it through the environment rather than inline `${{ }}` interpolation, so a
crafted filename cannot inject into the shell.

## The backstop gate

Per-job contexts are the enforcement. `ci-gate` survives alongside them as a
machinery backstop with `if: always()`, `needs:` on the gating concern callers,
and a step that fails on `failure` or `cancelled`.

It exists for one failure the per-job contexts structurally cannot catch. If a
`changes` detector fails, its dependent jobs skip, and GitHub counts a skipped
required check as passed. That is a silent enforcement bypass. A caller's
result aggregates its entire called workflow, so the gate sees the detector
failure that the per-job contexts cannot.

Advisory jobs that never fail are deliberately absent from its `needs`.

## Permissions and secrets

Top-level `permissions: contents: read`, with any job needing more declaring it
at the job level next to the reason.

Called workflows inherit `secrets.GITHUB_TOKEN` from the caller, so a concern
file needs no `secrets:` block for it. Anything else is passed explicitly; a
concern file never receives secrets it does not name.

## Setup and caching

`.github/actions/setup` is the single place the mise and vp pins live. Every
job calls it with the same tool list rather than computing a per-job subset:
uniform setup costs a few seconds of installing tools a job will not use, and
buys the property that moving a step between jobs is never accompanied by a
silent missing-tool failure.

The pin consolidation stays visible to Renovate because its custom manager for
annotated version pins matches composite action files as well as workflow
files.

Caching layers currently in use: the mise tool cache, the vp store cache, and a
build cache for the Rust plane. A cache saved by a pull request run is scoped
to that pull request, so treat every cache as saved when lucky and give each
one `restore-keys` prefix fallbacks.

## Adding a concern

1. Write `.github/workflows/<concern>.yml` as `workflow_call`-only, with a
   `changes` job and one job per check group.
2. Add the caller job to `ci.yml`, with no `if:` and no `name:`.
3. Add the caller to `ci-gate.needs`, unless every job in it is advisory.
4. Open the pull request and read the actual check names off it.
5. Add those exact strings to the branch ruleset as required contexts.

Step 4 is not optional. Add a context to the ruleset before a run has proven
the string, and a typo blocks every pull request until someone removes it by
hand.

## Repository-specific values

Everything above is portable. These are the parts that are not:

- Concerns: `lint` today, with the remaining planes moving across in sequence.
- Filters live in `.github/path-filters.yml` and are consumed by both CI and
  `scripts/preflight-scope.sh`.
- Tools come from `mise.toml`; the JS workspace resolves under the package
  manager `package.json` declares.
- `ci.yml` is named `CI`, and `sonar.yml` matches that string in a
  `workflow_run` trigger. Renaming one orphans the other silently.
