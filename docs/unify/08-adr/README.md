# Architecture Decision Records

- [0001-v1-scope.md](0001-v1-scope.md): V1 merges context planes, not agent orchestration
- [0002-multiple-ingestion-protocols.md](0002-multiple-ingestion-protocols.md): Use multiple ingestion protocols with one context plane
- [0003-storage-authority.md](0003-storage-authority.md): SQLite and durable artifacts are canonical; Qdrant and graph summaries are derived
- [0004-selective-observation-vectorization.md](0004-selective-observation-vectorization.md): Vectorize selected semantic units, not every observation row
- [0005-coarse-shared-crates.md](0005-coarse-shared-crates.md): Extract coarse reusable crates instead of mirroring Axon's 23 crates
- [0006-contract-machinery.md](0006-contract-machinery.md): Extend Soma xtask as the contract control plane
- [0007-context-broker-product-layer.md](0007-context-broker-product-layer.md): Keep the context broker in Soma's product layer initially
- [0008-graph-sqlite.md](0008-graph-sqlite.md): Use an evidence-first temporal graph backed by SQLite for v1
- [0009-ai-session-model.md](0009-ai-session-model.md): Use one typed AI-session model with dual projections
- [0010-existing-soma-surfaces.md](0010-existing-soma-surfaces.md): Keep existing Soma gateway, auth, provider catalog, and surfaces authoritative
- [0011-semantic-outbox.md](0011-semantic-outbox.md): Use a transactional semantic outbox
