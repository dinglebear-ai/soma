---
date: 2026-07-29 17:48:46 EST
repo: git@github.com:dinglebear-ai/soma.git
branch: docs/python-provider-platform-plan
head: a763fcefcb71de7d2b77a693aeae3779d8b5c13c
plan: docs/superpowers/plans/2026-07-29-python-persistent-supervised-runner.md
working directory: /home/jmagar/workspace/soma/.worktrees/docs-python-provider-platform-plan
worktree: /home/jmagar/workspace/soma/.worktrees/docs-python-provider-platform-plan
beads: rmcp-template-dox1, rmcp-template-dox1.1, rmcp-template-dox1.2, rmcp-template-dox1.3, rmcp-template-dox1.4, rmcp-template-dox1.5, rmcp-template-dox1.6
---

# Python persistent runner plan and implementation

> Follow-up reconciliation: this is a historical session record. The delivered
> production control transport is authenticated loopback TCP, not the
> stdin/stdout-pipe design recorded during planning. Current status and open
> gates live in
> [`docs/specs/python-provider-platform.md`](../specs/python-provider-platform.md).

## User Request

Review `docs/specs/python-provider-platform.md`, write an implementation plan,
perform an engineering review of that plan, revise it, and execute the entire
revised plan in one batch using the executing-plans workflow.

## Session Overview

The Python provider platform plan was reconciled with the live repository,
reviewed, rewritten, and implemented. The branch now contains an opt-in
supervised persistent Python runner, async candidate activation and retirement,
typed configuration, process-tree ownership, cross-language protocol fixtures,
documentation, and verification coverage.

## Sequence of Events

1. Inspected the platform specification, provider runtime, configuration,
   registry, Python SDK packaging, protocol fixtures, and existing tests.
2. Wrote the implementation plan, treated it as bead `rmcp-template-dox1`, and
   incorporated the engineering-review findings into an authoritative revision.
3. Implemented runner bootstrap, protocol negotiation, typed eligibility,
   supervision, async registry activation, generation retirement, tests, and
   operator documentation without stopping at intermediate plan tasks.
4. Ran focused and integration verification, resolved formatting and test
   issues, rebased onto the current remote base, and pushed the feature branch.
5. Closed all six child beads and the parent epic after the implementation and
   observed verification succeeded.

## Key Findings

- `python -I -m soma_provider.runner` requires the matching installed wheel;
  persistent startup therefore fails closed when the SDK is unavailable.
- Persistent providers cannot safely inherit provider/tool runtime environment
  declarations yet, so eligibility returns
  `python_persistent_env_unsupported`.
- Registry candidate preflight must occur outside synchronous registry locks;
  only ready describe-and-health-checked candidates are published.
- Worker timeout and retirement require descendant ownership, implemented with
  Unix process groups and Windows kill-on-close Job Objects.

## Technical Decisions

- One-shot execution remains the default and rollback mode; persistent execution
  is selected through typed `SOMA_PYTHON_RUNNER_*` configuration.
- An ephemeral loopback TCP connection authenticated with a per-launch token
  carries length-prefixed protocol frames on all platforms. Provider Python
  stdout is redirected to continuously drained, bounded stderr.
- Each worker accepts one active invocation. Concurrent calls receive
  `python_provider_busy` without entering an unbounded queue.
- Candidate refreshes are serialized and coalesced, while replaced providers
  drain asynchronously after the atomic registry swap.

## Files Changed

| Status | Path | Purpose |
|---|---|---|
| modified | `CHANGELOG.md` | Record the persistent runner feature. |
| modified | `Cargo.lock` | Record dependency changes. |
| modified | `apps/soma/src/bootstrap.rs` | Inject typed runner selection into production composition. |
| modified | `crates/shared/provider-adapters/Cargo.toml` | Add supervisor platform/runtime dependencies. |
| modified | `crates/shared/provider-adapters/src/python.rs` | Add persistent provider construction, eligibility, invocation, and retirement. |
| created | `crates/shared/provider-adapters/src/python/supervisor.rs` | Implement serial worker lifecycle, limits, restarts, teardown, and tests. |
| modified | `crates/shared/provider-adapters/src/python_protocol.rs` | Add negotiated bidirectional persistent message types. |
| modified | `crates/shared/provider-adapters/src/python_protocol_tests.rs` | Verify protocol negotiation and state. |
| modified | `crates/shared/provider-adapters/src/python_tests.rs` | Verify persistent eligibility. |
| modified | `crates/shared/provider-core/src/provider.rs` | Add provider retirement lifecycle hook. |
| modified | `crates/soma/api/src/api.rs` | Use async provider refresh from REST. |
| modified | `crates/soma/application/Cargo.toml` | Enable async refresh coordination. |
| modified | `crates/soma/application/src/app.rs` | Expose async refresh facade methods. |
| modified | `crates/soma/application/src/lib.rs` | Export runner types and async registry builders. |
| modified | `crates/soma/application/src/provider_registry.rs` | Preflight, atomically swap, coalesce, and retire generations. |
| modified | `crates/soma/application/src/providers/filesystem.rs` | Build eligible persistent providers and validate source files. |
| modified | `crates/soma/config/src/config.rs` | Add typed runner mode and limits. |
| modified | `crates/soma/config/src/config_tests.rs` | Verify runner configuration parsing and validation. |
| modified | `crates/soma/config/src/lib.rs` | Export runner configuration types. |
| modified | `crates/soma/mcp/src/rmcp_server.rs` | Await provider refresh on MCP surfaces. |
| modified | `crates/soma/mcp/src/rmcp_server/support.rs` | Route MCP refresh through the async application facade. |
| modified | `docs/PROVIDERS.md` | Document opt-in, limits, eligibility, and rollback. |
| modified | `docs/specs/python-provider-platform.md` | Reconcile phase status with the implementation. |
| created | `docs/superpowers/plans/2026-07-29-python-persistent-supervised-runner.md` | Preserve the reviewed implementation plan. |
| created | `packages/python/python/soma_provider/_runtime.py` | Share provider discovery and invocation runtime logic. |
| created | `packages/python/python/soma_provider/runner.py` | Implement the installed persistent worker. |
| modified | `packages/python/tests/runner_protocol_v1.json` | Update the cross-language invocation fixture. |
| created | `packages/python/tests/test_runner.py` | Exercise descriptor and stdio runner transports. |
| modified | `packages/python/tests/verify_installed.py` | Verify installed runner discoverability. |

## Beads Activity

| Bead | Action | Final status | Why |
|---|---|---|---|
| `rmcp-template-dox1` | created, reviewed, commented, closed | closed | Parent plan and review record. |
| `rmcp-template-dox1.1` | claimed and closed | closed | Runner bootstrap and child ownership. |
| `rmcp-template-dox1.2` | created and closed | closed | Negotiated protocol. |
| `rmcp-template-dox1.3` | created and closed | closed | Typed eligibility. |
| `rmcp-template-dox1.4` | created and closed | closed | Serial supervision. |
| `rmcp-template-dox1.5` | created and closed | closed | Async candidate activation. |
| `rmcp-template-dox1.6` | created and closed | closed | Parity, documentation, and gates. |

## Repository Maintenance

- The authoritative plan lives under `docs/superpowers/plans/`, not
  `docs/plans/`; it remains with the active specification rather than being
  moved by the generic completed-plan sweep.
- All directly related beads were verified closed. No follow-up bead was
  created because the requested implementation batch and its documented gates
  completed.
- Existing sibling worktrees were left untouched because several remain on
  unmerged feature branches and their ownership is outside this session.
- Provider and platform documentation contradicted by the new runtime path was
  updated in the implementation commit.

## Tools and Skills Used

- `superpowers:writing-plans`, `lavra:lavra-eng-review`, and
  `superpowers:executing-plans` for the requested plan-review-execution flow.
- Shell, Git, Cargo, uv, pytest, and GitHub CLI for inspection, builds, tests,
  branch synchronization, and publication.
- `apply_patch` for source and documentation edits.
- `bd` for plan and implementation tracking.
- An unrelated CI investigation was explicitly stopped when the user narrowed
  the scope back to full plan execution.

## Commands Executed

| Command | Result |
|---|---|
| `cargo test -p soma-provider-adapters --features python` | 84 tests passed. |
| `cargo test -p soma-application` | 109 unit tests and 2 integration tests passed. |
| `cargo test -p soma --test python_provider` | 25 tests passed. |
| `cargo test -p soma-config` | 44 tests passed. |
| `PYTHONPATH=python uv run --with pytest python -m pytest -q tests` | 16 tests and 11 subtests passed. |
| `cargo clippy -p soma-provider-adapters --features python --all-targets -- -D warnings` | Passed. |
| `cargo xtask check-docs` | Generated docs current. |
| `cargo check -p soma --all-features` | Passed after rebase. |
| `cargo fmt --all -- --check` and `git diff --check` | Passed. |

## Errors Encountered

- Initial Python test commands used paths relative to the wrong working
  directory; rerunning from `packages/python` with the correct paths resolved it.
- A native extension generated during wheel testing changed the pure-fallback
  expectation; the generated untracked artifact was removed from the worktree
  and source-path tests passed.
- The feature branch rebase produced three import-list conflicts caused by
  formatting changes on the newer base. The imports were merged, formatted, and
  verified before push.
- The first plain `git push` saw a mismatched upstream name; setting the feature
  branch upstream explicitly resolved it without force-pushing.

## Behavior Changes (Before/After)

| Area | Before | After |
|---|---|---|
| Python execution | Bounded one-shot bridge only. | One-shot default plus opt-in supervised persistent workers. |
| Candidate activation | Synchronous file refresh. | Async describe/health preflight and atomic publication. |
| Concurrency | New process per invocation. | One active call per persistent worker with stable busy rejection. |
| Failure recovery | Per-call process exit. | Process-tree teardown, later bounded restart, and quarantine. |
| Retirement | Provider replacement relied on drop behavior. | Explicit bounded drain and shutdown after registry swap. |

## Verification Evidence

| Command | Expected | Actual | Status |
|---|---|---|---|
| Adapter tests | Python adapter and supervisor pass | 84 passed | pass |
| Application tests | Registry and provider lifecycle pass | 111 passed | pass |
| Python provider integration | Existing compatibility remains | 25 passed | pass |
| Python SDK tests | Runner and protocol fixtures pass | 16 passed, 11 subtests | pass |
| Clippy | No warnings | Passed with `-D warnings` | pass |
| Docs and formatting | No generated-doc or formatting drift | Passed | pass |

## Risks and Rollback

- Persistent execution remains opt-in. Set
  `SOMA_PYTHON_RUNNER_MODE=one-shot` or remove the variable to roll back.
- Persistent mode intentionally rejects runtime environment declarations and
  does not yet provide Phase-8 broker capabilities.
- Python providers remain trusted code; process ownership and bounded I/O are
  lifecycle controls, not an OS sandbox.

## Decisions Not Taken

- Did not add actor scopes, trace forwarding, HTTP, secrets, state, metrics, or
  progress because the reviewed plan keeps those behind Phase 8.
- Did not introduce a multi-worker pool; the reviewed Phase-6 contract is
  deliberately serial.
- Did not touch the unrelated PR-check failures supplied mid-session after the
  user clarified that the implementation plan was the only active scope.

## References

- `docs/specs/python-provider-platform.md`
- `docs/adr/0013-python-provider-authoring-boundary.md`
- `docs/PROVIDERS.md`
- `docs/superpowers/plans/2026-07-29-python-persistent-supervised-runner.md`

## Next Steps

- Create the feature PR from `docs/python-provider-platform-plan`.
- Review the entire PR diff and address every introduced finding before merge.
- Keep Phase-8 capability brokering and containment as separate follow-on work.
