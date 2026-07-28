# Provider Examples

These files are examples only. Copy one into `./providers/` or point the runtime
at this directory:

```bash
SOMA_PROVIDER_DIR=./examples/providers soma providers list
```

The examples are intentionally outside the default `./providers` directory so
local development does not load sample actions by accident.

Python authoring examples live in `python/`: minimal, decorated Context,
async, Pydantic, LangChain, and LlamaIndex providers. Files with third-party
dependencies include PEP 723 metadata for the future uv environment manager.

Markdown files in this directory are exposed as MCP prompts. For example,
`code-review.md` appears as the `code-review` prompt.

`resources/` demonstrates the structured resources layout: `runbook.md` is a
static resource (`soma://resources/runbook`), and `service/[name].ts` is a
dynamic resource template (`soma://resources/service/{name}`) that requires
Node to actually read. See `docs/PROVIDERS.md`'s Resources section and
`docs/contracts/drop-in-provider-layout.md`.
