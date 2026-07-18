# soma-cli-core

Reusable command-line plumbing extracted from the Soma CLI: output-format
selection, JSON rendering, confirmation I/O primitives, and terminal/color
capability policy.

The crate owns generic terminal mechanics only. It does not depend on any
Soma product crate and knows nothing about Soma commands, action names,
scopes, or product exit-code policy — those stay in `soma-cli`.

```rust
use soma_cli_core::color::{green, red};
use soma_cli_core::terminal::stderr_supports_color;

let enabled = stderr_supports_color();
println!("{}  {}", green("ok", enabled), red("failed", false));
```

## What this crate owns

- common CLI flag-scanning primitives (`common_args`)
- output-format selection between human and JSON rendering (`output`)
- JSON pretty-printing helpers (`json`)
- confirmation I/O primitives — "type the name to confirm" prompts (`confirmation`)
- terminal capability detection and `NO_COLOR` / `--color` policy (`terminal`)
- ANSI color/style helpers, plus the Aurora CLI token palette as reusable
  defaults (`color`)

Table rendering, progress reporting, shell-completion generation, and
structured CLI error presentation are candidate future modules (see plan
§3.13) but are intentionally not included here yet — `soma-cli` has no
current caller for them, and this crate only carries mechanics an actual
consumer exercises. Add a module here alongside the PR that wires it up.

## What this crate does not own

- Soma's CLI parser or command set
- dynamic provider command projection
- mapping to application requests
- product exit-code policy
- business confirmation policy — this crate can prompt a human, but does not
  decide which operations require confirmation
