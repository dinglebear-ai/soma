# Existing Soma Foundation

## Audited shared crates on the recorded Soma main baseline

```text
auth
cli-core
codemode
codex-app-server-client
http-api
http-server
incus-client
mcp
observability
openapi
provider-adapters
provider-core
self-update
tauri-shell
traces
```

These crates remain authoritative within their current responsibilities.

## Audited Soma product crates

```text
api
application
cli
client
config
domain
integrations
mcp
palette
runtime
test-support
web
```

The context layer extends these product crates first through modules and use cases.

## Gateway status

Gateway architecture, OAuth, provider catalog, Code Mode, and surface projection are treated as final target architecture for this program. The recorded public `main` baseline may not include every gateway PR the user has in flight. The convergence program MUST re-audit the merged gateway baseline before implementation begins, but MUST NOT reopen its architecture merely because the branch state differs.

## No duplicate extraction

Do not create:

```text
context-auth
context-gateway
context-provider-catalog
context-codemode
context-mcp-surface
context-http-surface
context-cli-surface
context-web-framework
context-observability
context-self-update
```

New shared crates plug into the existing system.
