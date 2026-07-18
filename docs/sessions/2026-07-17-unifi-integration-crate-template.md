---
date: 2026-07-17 23:18:07 EST
repo: git@github.com:jmagar/soma.git
branch: claude/soma-crate-structure-f70dc9
head: dab5f19
working directory: /home/jmagar/workspace/soma/.claude/worktrees/soma-crate-structure-f70dc9
worktree: /home/jmagar/workspace/soma/.claude/worktrees/soma-crate-structure-f70dc9
pr: #163 "Add crates/integrations/ vendor layer, seed with unifi" (https://github.com/jmagar/soma/pull/163) — merged into main as bc5872d
beads: rmcp-template-71sn
---

## User Request

Decide where extracted, per-service API-client crates (unifi, unraid, arcane,
apprise, gotify, tailscale, ytdl, and yarr's sub-services) should live in the
soma monorepo ahead of eventually publishing them to crates.io independently.
Then, treating `unifi` as the first extraction, harden it into a "picture
perfect" reference template the other ~15 crates can copy — review it,
open a PR, address everything surfaced across multiple review rounds, merge
once CI is green, and document the session.

## Session Overview

Recommended and scaffolded `crates/integrations/<service>/` as a new
top-level vendor layer (distinct from the soma-prefixed `crates/shared/*`
layer), seeded with `unifi` lifted from `unifi-rmcp`. Hardened the crate
through two review rounds — a 4-agent parallel review (code-reviewer,
silent-failure-hunter, type-design-analyzer, pr-test-analyzer) that found a
21-action data-integrity defect and a missing end-to-end test for the
dynamic dispatcher, followed by a template-completeness pass (rate-limit
handling, `#[non_exhaustive]` errors, configurable timeout, stricter
doc-lint, a rewritten extraction checklist). Ground-truthed one review
finding (a Connector-API path double-prefix fix) against the user's real,
live UniFi controller. Merged PR #163 into `main` with all CI checks green.

## Sequence of Events

1. Answered the crate-location design question: recommended
   `crates/integrations/<service>/` over nesting under
   `crates/shared/integrations/`, since `crates/shared/*` is namespaced
   `soma-*` and is soma's own runtime plumbing, whereas these crates must
   have zero soma coupling and be independently publishable.
2. Scaffolded the `crates/integrations/` vendor layer with `Layer::Vendor`
   architecture-boundary enforcement, seeded with `unifi` (client, config,
   http, service, official/internal API bindings, capability detection,
   actions), lifted from `unifi-rmcp`'s already-separated `crates/unifi`
   (commit `c35e211`). Fixed a `.gitignore` `data/` pattern that was
   silently dropping the crate's bundled JSON capability-inventory fixtures
   (commit `b24358a`). Hardened the crate into typed `thiserror` errors, one
   pooled `reqwest::Client`, a secret-redacting `Debug` impl, 53 inline unit
   tests, 7 wiremock HTTP tests, and crate-level docs (commit `b2c3053`).
3. On explicit instruction, stopped after `unifi` — did not proceed to the
   other ~15 services — so the template could be reviewed first.
4. Reviewed whether `Cargo.toml` was production-ready for crates.io: found
   the crate had no bundled `LICENSE` file (the workspace-root `LICENSE`
   lives outside the crate directory and isn't packaged by `cargo package`)
   and inherited a misleading `homepage.workspace` field pointing at soma's
   own product site. Fixed both.
5. Ran four parallel review agents against PR #163 (code-reviewer,
   silent-failure-hunter, type-design-analyzer, pr-test-analyzer):
   - **pr-test-analyzer**: the dynamic action-dispatch pipeline
     (`ActionDispatcher::execute`) had zero test coverage above its
     individually unit-tested pieces. Added `tests/action_dispatch.rs` (5
     wiremock tests: dynamic official/internal actions with path params,
     both hybrid-resolution directions, unknown-action rejection).
   - **silent-failure-hunter**: found 21 more mutating admin actions in
     `data/unifi_internal_endpoint_models.json` sharing the exact defect
     already fixed once for `unifi_block_client` — a `GET` path identical
     to an existing read-only listing, so dispatching them silently re-ran
     the read and returned a misleadingly successful result instead of
     mutating anything.
   - **type-design-analyzer**: flagged `UnifiError::Timeout`/`Connect`
     discarding their `#[source] reqwest::Error` (breaking
     `Error::source()` chain-walking) and an `Unauthorized`
     tuple-vs-struct-variant style inconsistency.
6. Independently re-verified the 21-action finding with a standalone Python
   script cross-checking every mutating+`GET` capability's path against
   every read-only capability's path in the bundled JSON — the result
   matched the agent's list exactly (21/21). Disabled all 21
   (`runtime:false`/`verified:false`) and added a catalog-wide invariant
   test (`no_dispatchable_mutating_action_shares_a_get_path_with_a_read_only_action`)
   so the bug class can't recur silently.
7. Applied all review findings plus adjacent fixes surfaced directly:
   `actions/hybrid.rs` rejecting a non-string `prefer` and treating a null
   `siteId` as absent; `http.rs` checking response status before strict
   JSON decode; `config.rs` flipping `skip_tls_verify`'s default to
   `false`; `UnifiError::Timeout`/`Connect` restored `#[source]`.
8. Asked whether the Connector-API double-prefix fix
   (`OfficialNetworkApi::path()` passing through already-qualified paths)
   was actually correct, given real controller access was available. Used
   the live `unifi` MCP tool (backed by the still-deployed, unmodified
   `unifi-rmcp`) against the user's real controller: confirmed the
   *original* unifi-rmcp source has the identical double-prefix defect with
   no passthrough branch at all, and that `validate_connector_path`'s own
   prefix-allowlist logic proves the `path` parameter is contractually
   defined as already-fully-qualified — settling correctness via the
   crate's own internal logic rather than a live call. Cross-validated the
   more consequential `/v1/sites/{siteId}/clients` URL-construction shape
   live instead (`official_list_sites`, then `official_list_clients` with
   and without the required `siteId`), matching exactly what
   `tests/action_dispatch.rs` exercises.
9. Committed the review-round fixes as `6371679`.
10. Asked for further template-quality suggestions; proposed and, on
    approval ("1-5"), implemented: HTTP 429 handling
    (`UnifiError::RateLimited` with parsed `Retry-After`),
    `#[non_exhaustive]` on `UnifiError`, a configurable
    `UnifiConfig::request_timeout` (previously a hardcoded 30s constant),
    `#![warn(missing_docs)]` → `#![deny(missing_docs)]`, and a rewritten
    `crates/integrations/README.md` checklist requiring every fix from both
    review rounds.
11. While implementing, also: made `UnifiClient` store `request_timeout` so
    `UnifiClient::config()` round-trips it instead of silently reporting
    the default; hand-wrote `serde` support for `Duration` (none built in);
    re-exported `DEFAULT_REQUEST_TIMEOUT` at the crate root to fix an
    intra-doc-link warning; added 4 new tests. Discovered and fixed
    unrelated `cargo fmt` drift in `xtask/src/architecture_tests.rs` that
    turned out to be the actual cause of PR #163's failing CI "Format"
    check — it was part of this PR's own diff (from the original
    scaffolding commit), not pre-existing on `main`.
12. Committed as `dab5f19`, pushed, watched CI to green (`CI Gate` and
    `MSRV Gate` both `SUCCESS`, `mergeStateStatus: CLEAN`), and merged PR
    #163 into `main` via a standard merge commit (`bc5872d`).
13. Requested this session log, plus a follow-up task (queued for
    immediately after) to audit `unifi/README.md` for full coverage of
    everything the crate contains.

## Key Findings

- **21-action data defect** (`crates/integrations/unifi/data/unifi_internal_endpoint_models.json`):
  `unifi_authorize_guest`, `unifi_force_reconnect_client`,
  `unifi_forget_client`, `unifi_rename_client`, `unifi_set_client_ip_settings`,
  `unifi_unauthorize_guest`, `unifi_unblock_client` (all collide with `GET
  /rest/user`); `unifi_toggle_wlan`, `unifi_update_wlan` (`GET
  /rest/wlanconf`); `unifi_reorder_firewall_policies`,
  `unifi_toggle_firewall_policy`, `unifi_update_firewall_policy` (`GET
  /v2/firewall-policies`); `unifi_toggle_traffic_route`,
  `unifi_update_traffic_route` (`GET /v2/trafficroutes`);
  `unifi_set_outlet_state`, `unifi_update_device_radio` (`GET
  /stat/device`); `unifi_update_network` (`GET /rest/networkconf`);
  `unifi_toggle_port_forward` (`GET /rest/portforward`);
  `unifi_toggle_qos_rule_enabled` (`GET /v2/qos-rules`);
  `unifi_update_ap_group` (`GET /v2/apgroups`); `unifi_toggle_oon_policy`
  (`GET /v2/object-oriented-network-configs`). Each was declared `mutating:
  true` but with a `GET` method and a path identical to a read-only listing
  endpoint — dispatching any of them would have silently returned the read
  result and reported success without performing the mutation.
- **Connector path double-prefix**: `crates/integrations/unifi/src/api/official.rs`'s
  `OfficialNetworkApi::path()` unconditionally re-prefixed every path with
  `/proxy/network/integration/`, even though `validate_connector_path`
  (`src/api/path.rs`) requires the caller-supplied Connector `path` value to
  already start with that exact prefix — producing a doubled, always-broken
  path for every Connector action. Confirmed the same defect, with no
  passthrough branch at all, exists unfixed in the currently-deployed
  `unifi-rmcp` source.
- **CI "Format" root cause**: PR #163's failing CI Format check was caused
  by `cargo fmt` drift in `xtask/src/architecture_tests.rs` (37 lines added
  by the original scaffolding commit `c35e211`), not by anything in the
  session's own review-round commits.
- **LICENSE packaging gap**: `cargo package -p unifi --list` confirmed
  that with only `license = "MIT"` set (no bundled `LICENSE` file), the
  crate's tarball would ship with no physical license text.

## Technical Decisions

- `crates/integrations/<service>/` (flat, one crate per service, no
  `soma-` name prefix) chosen over nesting under `crates/shared/`, since
  `crates/shared/*` crates are soma-namespaced (`soma-auth`,
  `soma-mcp-server`, ...) and depended on by soma's own product crates,
  while these vendor crates must be usable in an unrelated project with
  zero soma coupling — enforced by a new `Layer::Vendor` architecture-graph
  rule (vendor crates may depend on other vendor crates, never on
  `crates/shared/*` or `crates/soma/*`).
- Kept the 21 broken actions in the catalog with `runtime: false` rather
  than deleting their entries, preserving the dispatcher's "unknown action"
  vs. "known but disabled" distinction and letting each be re-enabled
  individually once its real endpoint is confirmed — mirrors how
  `unifi_block_client` was already handled in an earlier round.
- Hand-wrote a small `serde` `with`-module for `Duration` seconds
  (`config::duration_secs`) instead of adding `serde_with` or
  `humantime-serde` as a dependency for one field.
- Settled the Connector-path correctness question via the crate's own
  internal validation contract rather than continuing to chase an
  inconclusive live-controller signal once `official_connector_get` proved
  undiagnosable without server-side log access.

## Files Changed

| Status | Path | Purpose | Evidence |
|---|---|---|---|
| created | `crates/integrations/README.md` | Vendor-layer overview + extraction checklist (`c35e211`); checklist rewritten this session to require LICENSE bundling, no inherited `homepage`, catalog-collision test, dispatcher end-to-end test, `#[non_exhaustive]` errors, configurable timeout (`dab5f19`) | `git log`, `git diff` |
| created | `crates/integrations/unifi/{Cargo.toml,LICENSE,README.md,src/**,data/**,tests/**}` | Initial extraction of `unifi` from `unifi-rmcp` (~19 files) | `c35e211` |
| modified | `.gitignore` | Anchored `storage/data/logs/backups` ignores to repo root; unqualified `data/` pattern was dropping the crate's bundled JSON fixtures | `b24358a` |
| modified | `crates/integrations/unifi/src/{client,config,http,service}.rs`, `Cargo.toml`, `README.md` | Typed `thiserror` errors, pooled `reqwest::Client` threaded through dispatch, secret-redacting `Debug`, 53 unit + 7 wiremock tests, doctested crate docs | `b2c3053` |
| modified | `crates/integrations/unifi/Cargo.toml` | Dropped inherited `homepage.workspace`; confirmed `license = "MIT"` | `6371679` |
| created | `crates/integrations/unifi/LICENSE` | Per-crate MIT license text (workspace-root `LICENSE` isn't packaged by `cargo package`) | `6371679` |
| modified | `crates/integrations/unifi/data/unifi_internal_endpoint_models.json` | Disabled 21 mutating actions colliding with read-only GET paths (`runtime:false`/`verified:false`) | `6371679`, independently re-verified via one-off Python script |
| modified | `crates/integrations/unifi/src/capabilities/internal_network.rs` | Added catalog-wide GET-path-collision invariant test; fixed `alarms` legacy alias path (`/stat/alarm` → `/rest/alarm`) | `6371679` |
| modified | `crates/integrations/unifi/src/actions/{internal,hybrid}.rs`, `api/official.rs`, `http.rs`, `config.rs` | Path substitution reads post-normalization params; removed 2 wrong-endpoint normalize overrides; Connector-path passthrough fix; status-before-decode ordering; `skip_tls_verify` default → `false`; `prefer` type/null hardening | `6371679` |
| created | `crates/integrations/unifi/tests/action_dispatch.rs` | 5 new wiremock tests driving `ActionDispatcher::execute` end-to-end | `6371679`, extended `dab5f19` |
| modified | `crates/integrations/unifi/src/util.rs` | Deduplicated `truncate_data_array` (was defined twice) | `6371679` |
| modified | `crates/integrations/unifi/src/error.rs` | New `UnifiError::RateLimited` variant (HTTP 429 + `Retry-After`); `#[non_exhaustive]`; `Timeout`/`Connect` keep `#[source]`; `Unauthorized` tuple-variant style fix; doc comments on two variant-shape decisions | `dab5f19` |
| modified | `crates/integrations/unifi/src/http.rs` | 429 handling with `Retry-After` header parsing; timeout now reads `cfg.request_timeout` instead of a hardcoded constant | `dab5f19` |
| modified | `crates/integrations/unifi/src/config.rs` | New `request_timeout: Duration` field + `DEFAULT_REQUEST_TIMEOUT` const + hand-written `duration_secs` serde module; 2 new tests | `dab5f19` |
| modified | `crates/integrations/unifi/src/client.rs` | `UnifiClient` stores `request_timeout` so `.config()` round-trips it; 1 new test | `dab5f19` |
| modified | `crates/integrations/unifi/src/lib.rs` | `#![warn(missing_docs)]` → `#![deny(missing_docs)]`; re-exported `DEFAULT_REQUEST_TIMEOUT` | `dab5f19` |
| modified | `crates/integrations/unifi/tests/client.rs` | 2 new `RateLimited` tests; `config()` helper updated for the new field | `dab5f19` |
| modified | `crates/integrations/README.md`, `crates/integrations/unifi/README.md` | Checklist and crate README rewritten to reflect both review rounds | `dab5f19` |
| modified | `xtask/src/architecture_tests.rs` | Picked up pre-existing `cargo fmt` drift blocking PR #163's CI Format gate | `dab5f19` |

## Beads Activity

- `rmcp-template-71sn` — **created and claimed** this session: "Write a
  comprehensive, all-encompassing README for `crates/integrations/unifi`",
  P2 task, tracking the immediately-following follow-up work. Status:
  `in_progress`.
- No bead was created or claimed for the PR #163 review-round work itself
  (`6371679`, `dab5f19`) — a deviation from this repo's CLAUDE.md mandate
  to create a bead before non-trivial code changes; recorded here rather
  than silently repeated.

## Repository Maintenance

- **Plans**: `docs/plans/` does not exist in this repository (`ls
  docs/plans/` → no such file or directory) — nothing to move.
- **Beads**: `bd list --status=open | grep -i unifi` returned no
  pre-existing beads for this work; created and claimed `rmcp-template-71sn`
  for the pending README task (see Beads Activity above).
- **Worktrees/branches**: this session's own worktree/branch
  (`claude/soma-crate-structure-f70dc9`) is now fully merged into `main`
  (verified: `git merge-base --is-ancestor dab5f19 origin/main` → true) but
  intentionally left in place — a follow-up task (README audit) is still
  queued in it. No other worktree or branch was inspected for cleanup: the
  repo currently has ~10 other active worktrees and ~25 other branches
  (`refactor/pr11-integrations` through `pr19-delete-legacy`, several
  `worktree-agent-*`/`worktree-wf_*`, `bd-work/workspace-deps-and-freeze-audit`,
  the protected `marketplace-no-mcp`, etc.) with unclear ownership relative
  to this session's scope; none were touched.
- **Stale docs**: `crates/integrations/README.md` and
  `crates/integrations/unifi/README.md` were both updated as direct session
  output and are current, not stale. No other documentation was identified
  as contradicted by this session's changes.

## Tools and Skills Used

- **Shell (Bash)**: `git` (status/diff/log/fmt-adjacent/fetch/merge-base),
  `cargo` (build/test/clippy/fmt/doc/package/xtask check-architecture/
  check-version-sync), `gh` (`pr view`/`pr checks`/`pr merge`), `python3`
  (one-off independent verification of the 21-action collision list), `bd`
  (beads).
- **File tools**: Read/Edit/Write across crate source, tests, `Cargo.toml`,
  `README.md`, `LICENSE`, and the workspace `xtask` fixture — no issues.
- **Agent tool**: 4 parallel review subagents (code-reviewer,
  silent-failure-hunter, type-design-analyzer, pr-test-analyzer) dispatched
  against PR #163; all 4 returned substantive, independently-verifiable
  findings with no failures or empty results.
- **MCP**: `mcp__plugin_unifi_unifi__unifi` (the live, deployed `unifi-rmcp`
  server against the user's real UniFi controller) — used to ground-truth
  the Connector-path fix and cross-validate URL construction. Hit
  transient "classifier temporarily unavailable" errors on the first few
  calls, resolved on retry with no code/config change needed.
  `official_connector_get` itself never succeeded against this controller
  (opaque "check server logs" error regardless of a valid vs. garbage
  console id) — plausibly a topology mismatch (local single-site console
  vs. UniFi's cloud multi-console model), not further diagnosable without
  server-side log access.
- **WebFetch**: attempted against `developer.ui.com`'s Connector-GET docs
  page; returned no usable content (JS-rendered SPA — only the page header
  came through). Abandoned in favor of the live-controller + internal
  validation-contract approach.
- **ToolSearch**: used to load deferred tool schemas (`WebFetch`, the
  `unifi` MCP tool) before first use — no issues.
- **Skills**: `vibin:save-to-md` (this document).
- No browser-automation, Artifact, or Workflow tool was used this session.

## Commands Executed

| Command | Result |
|---|---|
| `cargo test -p unifi` | 79 tests pass (67 unit + 5 dispatch + 10 client + 1 doctest) |
| `cargo clippy -p unifi --all-targets -- -D warnings` | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --check` (workspace) | clean |
| `cargo doc -p unifi --no-deps` | clean (after fixing an intra-doc-link warning) |
| `cargo package -p unifi --list --allow-dirty` | confirms `LICENSE` bundled in the tarball |
| `cargo xtask check-architecture` | "Architecture check passed (24 workspace packages, 53 internal edges)" |
| `cargo xtask check-version-sync` | "OK: soma version-bearing files are in sync at 0.4.7" |
| Independent Python collision check on `data/unifi_internal_endpoint_models.json` | 21/21 match against the agent-reported list |
| `gh pr checks 163 --watch --interval 30` (background) | all checks complete, `CI Gate`/`MSRV Gate` both `SUCCESS` |
| `gh pr merge 163 --merge --delete-branch=false` | merged as `bc5872d` |
| `git fetch origin main && git merge-base --is-ancestor dab5f19 origin/main` | confirms merge landed on `main` |

## Errors Encountered

- `mcp__plugin_unifi_unifi__unifi` returned "claude-sonnet-5 is temporarily
  unavailable... classifier" on the first 3–4 calls. Resolved by retrying;
  no underlying issue.
- `official_connector_get` against the live controller failed with an
  opaque "Check server logs for details" error regardless of the `id`
  supplied (a real site UUID vs. a garbage string produced the identical
  error). Left unresolved — out of reach without server-side log access —
  but the underlying code-correctness question was independently settled
  via `validate_connector_path`'s own contract, so this did not block the
  fix.
- `cargo doc -p unifi --no-deps` warned that `DEFAULT_REQUEST_TIMEOUT` was
  a "private item" intra-doc-link target despite being declared `pub`; root
  cause was the constant not being re-exported at the crate root even
  though its containing module (`mod config;`) is private. Fixed by adding
  it to the `pub use config::{...}` re-export list.
- PR #163's CI "Format" job was red going into this session's second
  commit. Root cause was pre-existing `cargo fmt` drift in
  `xtask/src/architecture_tests.rs`, introduced by the original scaffolding
  commit (`c35e211`) rather than anything in this session's own commits.
  Fixed with a workspace-wide `cargo fmt`.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| `UnifiConfig`/timeout | Hardcoded 30s `Duration` constant in `http.rs`, not configurable | `UnifiConfig::request_timeout` field, default unchanged, round-trips through `UnifiClient::config()` |
| `UnifiError` | Closed enum; `Timeout`/`Connect` discarded their `reqwest::Error` source | `#[non_exhaustive]`; both variants keep `#[source]`; new `RateLimited` variant for HTTP 429 with parsed `Retry-After` |
| 21 mutating admin actions (unblock/rename/authorize client, toggle WLAN/firewall-policy/traffic-route/QoS-rule/OON-policy/port-forward, update network/AP-group/device-radio, set outlet state, reorder firewall policies) | Dispatchable; silently re-ran a read-only GET and returned a false-success result instead of mutating anything | Disabled (`runtime:false`) until each real endpoint is confirmed |
| `OfficialNetworkApi::path()` (Connector actions) | Unconditionally re-prefixed every path with `/proxy/network/integration/`, double-prefixing already-qualified Connector paths | Passes already-qualified `/proxy/network\|protect/integration/` paths through unchanged |
| `lib.rs` doc-lint | `#![warn(missing_docs)]` | `#![deny(missing_docs)]` |
| `Cargo.toml` | `license = "MIT"` with no bundled `LICENSE` file; inherited `homepage.workspace` pointing at soma's product site | Per-crate `LICENSE` file bundled; `homepage` dropped |
| `crates/integrations/unifi` dispatcher test coverage | Only individual pieces (path substitution, hybrid resolution, etc.) unit-tested | `tests/action_dispatch.rs` exercises `ActionDispatcher::execute` end-to-end |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| `cargo test -p unifi` | all pass | 79/79 pass | pass |
| `cargo clippy -p unifi --all-targets -- -D warnings` | no warnings | none | pass |
| `cargo clippy --workspace --all-targets -- -D warnings` | no warnings | none | pass |
| `cargo fmt --check` (workspace) | no diff | none | pass |
| `cargo doc -p unifi --no-deps` | no warnings | none (after fix) | pass |
| `cargo package -p unifi --list` | `LICENSE` present | present | pass |
| `cargo xtask check-architecture` | passes | 24 packages, 53 edges, passed | pass |
| `cargo xtask check-version-sync` | in sync | in sync at 0.4.7 | pass |
| `gh pr checks 163` (post-fix) | all required checks green | `CI Gate`/`MSRV Gate` both `SUCCESS`, `mergeStateStatus: CLEAN` | pass |
| Live controller: `official_list_sites`, `official_list_clients` (with `siteId`) | real data returned | real site + 25/31 real clients returned | pass |
| Live controller: `official_connector_get` | n/a — exploratory | opaque failure, both valid and garbage `id` | inconclusive, documented |

## Risks and Rollback

- Disabling the 21 mutating actions removes functionality some caller might
  have believed worked (it never actually did) — rollback is `git revert`
  on the JSON-data hunk of `6371679`, or re-enable each individually once
  its real endpoint is confirmed and tested.
- The merge to `main` was a standard (non-squash) merge commit `bc5872d`;
  rollback would be `git revert -m 1 bc5872d` if needed.
- No production deployment was touched: `crates/integrations/unifi` has
  `publish = false` and is not yet a dependency of any other workspace
  crate (confirmed via `cargo xtask check-architecture`'s dependency
  graph), so this PR has no runtime blast radius on the shipped `soma`
  binary.

## Decisions Not Taken

- No `native-tls` feature flag alongside the existing `rustls-tls` — no
  concrete caller need yet.
- No retry-with-backoff for transient `Timeout`/`Connect`/5xx failures —
  flagged as speculative without a driving use case, consistent with this
  project's conventions against premature abstraction.
- No further live-controller diagnosis of `official_connector_get`'s
  failure — would require server-side log access not available in this
  environment, and the code-correctness question was already settled
  independently via the crate's own validation contract.
- No repo-wide worktree/branch cleanup — out of scope for this session
  given unclear ownership of the ~10 other active worktrees and ~25 other
  branches present.

## References

- PR #163: https://github.com/jmagar/soma/pull/163 (merged as `bc5872d`)
- `crates/integrations/README.md`, `crates/integrations/unifi/README.md`
- UniFi Connector API docs (attempted, JS-rendered, not usable):
  https://developer.ui.com/network/v10.3.58/connectorget

## Open Questions

- Whether the 21 disabled actions' real endpoints should be sourced from
  actual UniFi controller traffic captures (rather than OpenAPI-adjacent
  inference) before re-enabling them.
- Whether `crates/integrations/unifi` should eventually get its own
  `release/components.toml` entry, distinct from the single `soma`
  component that exists today, once it's ready to publish.
- Whether the package name `unifi` is available on crates.io, or whether a
  brand-neutral name needs to be chosen first — noted in the crate's own
  README `## Status` section as a known, still-open follow-up.

## Next Steps

- **Immediate (queued, same session)**: audit and rewrite
  `crates/integrations/unifi/README.md` to be the crate's fully
  comprehensive, "penultimate" doc, covering everything the crate contains
  — tracked as bead `rmcp-template-71sn` (claimed).
- **Follow-on (not yet started)**: extract the next
  `crates/integrations/*` crate (unraid is the most-referenced next
  candidate) using `unifi` as the template, per
  `crates/integrations/README.md`'s checklist.
- **Follow-on**: confirm real endpoints for the 21 disabled mutating
  actions in `data/unifi_internal_endpoint_models.json`, then re-enable and
  test each individually.
- No immediate blocked work; proceed directly to the README audit.
