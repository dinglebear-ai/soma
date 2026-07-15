# Soma Code Mode/OpenAPI Port

## Summary

Implemented self-contained `soma-openapi` and `soma-codemode` ports from Lab,
with `soma-codemode/openapi` as the only dependency edge between them.

## Commits

- `926cad2` `chore: scaffold standalone codemode and openapi crates`
- `8f675a8` `feat(openapi): port self-contained config and registry core`
- `2eb735c` `feat(openapi): port hardened HTTP dispatch`
- `4e29f9b` `feat(codemode): port support primitives and protocol`
- `a08c200` `feat(codemode): port runner and pool core`
- `97eb4c4` `feat(codemode): port local providers`
- `0ebada0` `feat(codemode): gate openapi integration`
- `HEAD` `docs: record codemode openapi port verification`

## Verification

| Command | Result | Duration | Cache mode |
|---|---|---:|---|
| `git fetch --prune origin` | pass | 0.35s | network |
| `cargo fmt --all -- --check` | pass | 0.34s | warm |
| `cargo test -p soma-openapi` | pass, 54 unit tests, live smoke compile with 1 ignored network test | 0.58s | warm |
| `cargo test -p soma-codemode --no-default-features` | pass, 109 unit/bin tests | 2.25s | warm |
| `cargo test -p soma-codemode --features openapi` | pass, 114 unit/bin tests | 2.18s | warm |
| `cargo clippy -p soma-openapi -p soma-codemode --all-targets --all-features -- -D warnings` | pass | 6.11s | warm |
| `cargo tree -p soma-openapi` | pass | recorded during run | warm |
| `cargo tree -p soma-codemode --no-default-features` | pass | recorded during run | warm |
| `cargo tree -p soma-codemode --features openapi` | pass | recorded during run | warm |
| `cargo tree -p soma-openapi \| rg 'labby-\|soma-' \| rg -v '^soma-openapi '` | pass, no forbidden dependency hits beyond crate root | recorded during run | warm |
| `cargo tree -p soma-codemode --no-default-features \| rg 'labby-\|soma-openapi\|soma-(...)'` | pass, no matches | recorded during run | warm |
| `cargo tree -p soma-codemode --features openapi \| rg 'soma-openapi'` | pass, intended optional edge present | recorded during run | warm |
| `cargo tree -p soma-codemode --features openapi \| rg 'labby-\|soma-(...)'` | pass, no forbidden existing Soma/Lab matches | recorded during run | warm |
| `find crates/soma-codemode crates/soma-openapi -type f -name '*.rs' ... wc -l ...` | pass, no file over 500 LOC | 0.05s | warm |
| `test -z "$(find crates/soma-codemode crates/soma-openapi -name mod.rs -print -quit)"` | pass | 0.00s | warm |
| `cargo xtask check-test-siblings` | pass | 0.11s | warm |
| `cargo xtask patterns` | pass with pre-existing warnings outside new crates | 0.33s | warm |
| `cargo xtask generate-provider-surfaces --check` | pass | 0.28s | warm |
| `cargo test -p soma --test workflow_shapes --all-features` | pass, 3 tests | 4.65s | warm |
| `cargo test -p xtask release_versions --all-features` | pass, 13 filtered tests | 4.77s | warm |
| `cargo test --workspace` | pass | ~98s | warm |
| `cargo xtask check-version-sync` | pass | 0.40s | warm |
| `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr` | pass | 0.13s | warm |
| `cargo xtask release-plan --head HEAD --mode main --json` | pass | 0.53s | warm |
| cold `cargo test -p soma-openapi` | pass, 54 unit tests, live smoke compile with 1 ignored network test | 34.38s | cold |
| cold `cargo test -p soma-codemode --no-default-features` | pass, 109 unit/bin tests | 22.61s | cold |
| cold `cargo test -p soma-codemode --features openapi` | pass, 114 unit/bin tests | 23.82s | cold |
| post-rebase `cargo fmt --all -- --check` | pass | 0.61s | warm |
| post-rebase `cargo test -p soma-openapi` | pass, 54 unit tests, live smoke compile with 1 ignored network test | 4.81s | warm |
| post-rebase `cargo test -p soma-codemode --no-default-features` | pass, 109 unit/bin tests | 3.17s | warm |
| post-rebase `cargo test -p soma-codemode --features openapi` | pass, 114 unit/bin tests | 6.54s | warm |
| post-rebase `cargo clippy -p soma-openapi -p soma-codemode --all-targets --all-features -- -D warnings` | pass | 37.55s | warm |
| post-rebase `cargo test -p soma --test architecture_boundaries codemode_openapi -- --nocapture` | pass, 4 filtered boundary tests | 34.12s | warm |

## Notes

- `soma-openapi` intentionally hardens beyond Lab by rejecting IPv4 Class E and
  IPv6 multicast.
- `soma-codemode --no-default-features` does not link `soma-openapi` or
  `reqwest`.
- No Lab crates or existing Soma crates are dependencies of the standalone
  crates.
- The plan's negative `soma-openapi` tree grep was adapted to exclude the root
  package line, because `cargo tree -p soma-openapi` necessarily prints
  `soma-openapi` as the first line.

## Post-review addendum

- Replaced the incomplete in-process broker path with a real parent-side
  subprocess bridge. The broker now carries caller, surface, scope,
  execution_id, and UI capture into the runner request, and the JS wrapper
  exposes `callTool`, `codemode.*`, snippets, steps, and artifact writes over the
  framed runner protocol.
- Tightened runner executable resolution so library callers spawn the actual
  `soma-codemode-runner` binary via `SOMA_CODE_MODE_RUNNER_EXE` or side-by-side
  discovery instead of accepting any executable as the current process.
- Hardened OpenAPI path handling after review: allowed operation path templates
  must start with `/`, reject backslashes, reject `.` / `..` segments, and base
  path containment is component-aware.
- Split state edit-plan handling into `state/workspace_edit.rs` and added
  sibling tests, keeping all `soma-codemode` and `soma-openapi` Rust files under
  the hard 500 LOC cap.
- Added trace redaction for tool params/results and replaced provider-shaped
  secret fixtures with neutral redaction canaries.

Final verification after the post-review fixes:

- `cargo test -p soma-openapi`: pass, 58 unit tests plus ignored live Petstore
  network smoke compiled.
- `cargo test -p soma-codemode --no-default-features`: pass, 118 lib tests plus
  runner binary test.
- `cargo test -p soma-codemode --features openapi`: pass, 123 lib tests plus
  runner binary test.
- `cargo clippy -p soma-openapi -p soma-codemode --all-targets --all-features -- -D warnings`:
  pass.
- `cargo test --workspace`: pass.
- Cold-cache proof with fresh `CARGO_TARGET_DIR`, `RUSTC_WRAPPER=`, and
  `CARGO_BUILD_RUSTC_WRAPPER=` passed for `soma-openapi`,
  `soma-codemode --no-default-features`, and `soma-codemode --features openapi`.
- Soma gates passed: `cargo xtask check-test-siblings`,
  `cargo xtask test-soma-features`, `cargo xtask patterns`,
  `cargo xtask check-openapi --check`, `cargo xtask check-schema-docs --check`,
  `cargo xtask check-provider-manifest-contract`,
  `cargo xtask check-scaffold-intent-contract`, `cargo xtask check-stale-claims`,
  `cargo xtask check-version-sync`, and
  `cargo xtask check-release-versions --base origin/main --head HEAD --mode pr`.

## CI rescue addendum

- The first pushed run exposed a pre-existing full-history gitleaks finding in
  commit `03ec0523ed3aa7d428e73bac3afff14034de0b5a`, where the removed
  `crates/rtemplate-auth/src/authorize.rs` template contained a static RSA test
  fixture. Converted the repo gitleaks config from deprecated `[allowlist]` to
  current `[[allowlists]]` syntax and allowed that historical fixture commit.
- Verified the same full-history scan locally with `gitleaks detect --redact
  --verbose`: pass, 393 commits scanned, no leaks found.
- Reproduced CI's workspace clippy lane with `cargo clippy --all-targets -- -D
  warnings`: pass.
- Verified TOML formatting with `taplo check`: pass.

## CI runner addendum

- The fresh pushed CI run then exposed a Linux runner dependency issue:
  `javy 7.0.0` unconditionally enabled `rquickjs/bindgen`, which made
  `rquickjs-sys` require `libclang` during the Test, Clippy, and MSRV jobs.
- Removed the external `javy` dependency, depended on `rquickjs` directly
  without the `bindgen` feature, and inlined the tiny Javy compatibility surface
  used by the runner (`Runtime`, `Config`, `Args`, `to_js_error`, and
  `val_to_string`) inside `soma-codemode`.
- Verified `cargo tree -p soma-codemode -e features` has no `javy` or
  `rquickjs/bindgen` path.
- Re-ran local proof after the fix: `cargo test -p soma-codemode
  --no-default-features`, `cargo test -p soma-codemode --features openapi`,
  `cargo clippy --all-targets -- -D warnings`, `cargo build --bin soma`,
  `cargo nextest run --profile ci` (1072 passed, 1 skipped), `cargo fmt --all
  -- --check`, `cargo xtask check-test-siblings`, full-history `gitleaks detect
  --redact --verbose` (394 commits, no leaks), `taplo check`, and the 500 LOC
  gate for `soma-codemode`/`soma-openapi`.
