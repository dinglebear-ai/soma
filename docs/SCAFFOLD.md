# Frictionless Scaffolding

`cargo xtask scaffold` is the front door for starting a new rmcp-family server.
It bridges the MCP `scaffold_intent` JSON, `cargo-generate`, the Rust
post-processor, action starter snippets, and generated-project verification.

## Plan first

Use a lightweight name-based plan when you already know the target service:

```bash
cargo xtask scaffold --name myservice --category upstream-client --port auto --plan
```

Use scaffold intent JSON when the MCP elicitation wizard collected the details:

```bash
cargo xtask scaffold --intent scaffold-intent.json --plan
```

Plans are side-effect free. They print the `cargo-generate` values, selected
surfaces, default Cargo features, runtime choices, plugin choices, research
inputs, and remaining human work.

## Generate

After reviewing the plan, generate into an output parent directory:

```bash
cargo xtask scaffold --intent scaffold-intent.json --apply ../generated
```

The command runs `cargo generate`, applies `cargo xtask cargo-generate-post`,
writes `docs/scaffold-report.md` in the generated project, verifies generated
shape, and runs `cargo check --workspace --all-targets` unless
`--no-cargo-check` is passed.

## Verify

Before publishing or committing a generated project, prove the scaffold shape:

```bash
cargo xtask scaffold --verify ../generated/myservice-mcp
```

The verifier rejects copied template-only files, plugin manifests containing a
`version` key, and missing `AGENTS.md` / `GEMINI.md` symlinks when `CLAUDE.md`
exists. By default it also runs `cargo check --workspace --all-targets`; pass
`--no-cargo-check` for a static-only check while iterating.

## Adapt

After generation, print a path-aware adaptation checklist for the generated
project:

```bash
cargo xtask scaffold --adapt-plan ../generated/myservice-mcp
```

The adapt plan reads `docs/scaffold-report.md` when present, infers the selected
profile and surfaces, and prints the concrete files to update for service
implementation, action wiring, optional API/Web/plugin surfaces, tests, and
verification. It is read-only and does not mutate the generated project.

## Write action starters

Use the same action manifest to materialize starter artifacts in a generated
project:

```bash
cargo xtask scaffold \
  --write-action-starters ../generated/myservice-mcp \
  --actions actions.json
```

This writes `docs/action-starters/` with reviewable snippets for action
metadata, MCP dispatch, CLI variants, service stubs, and test coverage. The
command intentionally does not patch source files directly; generated projects
can have custom names and partially adapted code, so reviewable snippets are the
safe automation boundary.

## Lean upstream-client default

`upstream-client` projects default to `local-adapter`, which keeps the generated
binary focused on CLI + stdio MCP. Use `application-platform` when the project
owns API/Web workflows and should default to the full platform feature set.

## Action starter manifest

Provide an optional action manifest to generate source snippets for the
business-action boilerplate:

```bash
cargo xtask scaffold \
  --intent scaffold-intent.json \
  --actions actions.json \
  --plan
```

Example:

```json
{
  "actions": [
    {
      "name": "list_things",
      "description": "List visible things.",
      "scope": "read",
      "params": [
        { "name": "kind", "type": "string", "required": false }
      ]
    }
  ]
}
```

The snippets are intentionally starter code. Keep the thin-shim rule: business
logic belongs in the service layer, while MCP and CLI shims only parse input and
dispatch.

## Research inputs

If the intent includes `crawl_docs`, the plan lists the approved URLs, repos, or
search topics. Run the Axon research/crawl step before replacing the stub client;
do not invent upstream API behavior from the scaffold request alone.
