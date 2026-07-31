# Python Provider Threat Model

Status: implemented security boundary for Python provider phases 8-10.

## Assets and trust boundaries

Soma protects the host process, provider credentials, actor authority, state
namespaces, environment caches, registry generations, protocol integrity, and
component artifacts. Provider source and dependencies are untrusted in
`brokered` mode. They are explicitly trusted in `trusted` mode.

The private runner socket is a control boundary, not an authorization source.
Every host call is bound to the active invocation ID and intersected with:

1. the provider manifest declaration;
2. deployment allowlists;
3. authenticated actor scopes;
4. the service available on this host.

## Execution profiles

- `disabled`: rejects Python execution.
- `trusted`: preserves the compatibility posture and service-account authority.
- `brokered`: Linux-only enforcement requires Bubblewrap, libseccomp, prlimit,
  and a delegated cgroup-v2 root in `SOMA_PYTHON_BROKER_CGROUP_ROOT`.
  Missing enforcement fails closed with
  `python_brokered_containment_unavailable`.

Brokered Linux workers receive new user, PID, IPC, UTS, cgroup, mount, and
network namespaces; a read-only host filesystem view; a private `/proc` and
`/dev`; no ambient network namespace; seccomp denial of network creation,
mount, ptrace, BPF, keyring, performance, and userfault operations; address
space, file, process, descriptor, CPU, memory, and PID limits; and complete
process-tree cleanup. Their only host channel is the authenticated Unix control
socket. Windows workers always use kill-on-close Job Objects. Brokered mode on
platforms without equivalent ambient-authority enforcement fails closed.

## Capability-specific controls

- HTTP is HTTPS-only, rejects URL credentials and redirects, pins requests to
  the public DNS answers checked before connection, and rejects private,
  loopback, link-local, documentation, multicast, and unspecified addresses.
- Secret handles require provider and deployment allowlists. Secret values are
  returned only over the private channel and never included in audit records.
- State keys are prefixed by the declared namespace; writes require write
  declaration and `soma:write`.
- Logs, metrics, and progress require explicit declarations and deployment
  grants. Retained diagnostics are bounded and redact secret-like material.
- Cancellation has a cooperative query and a non-cooperative process-tree
  termination fallback.

## Threats and mitigations

| Threat | Mitigation |
|---|---|
| Malicious provider source | Brokered OS containment and default-deny host calls |
| Dependency compromise | Immutable `uv` lock/cache identity, wheel digest checks, offline reopening |
| Protocol spoofing | Per-launch token, bounded frames, request and invocation ID matching |
| Confused deputy | Provider declaration + deployment grant + actor-scope intersection |
| DNS rebinding / redirect escape | Resolve and validate every address, pin resolution, disable redirects |
| Secret leakage | Handle allowlists, bounded redaction, secret-free audit events |
| Cross-provider state access | Explicit namespace and namespaced storage keys |
| Cache substitution | Content digests, readiness metadata, immutable publication |
| Infinite native/Python work | timeout, cgroup/rlimit ceilings, process-tree kill |
| Infinite Wasm work | fuel plus Wasmtime epoch interruption and store resource limits |
| Component substitution | component validation, content-addressed candidates, atomic activation |

## Residual risks

Trusted mode intentionally retains ambient authority. Dependency installation
occurs before brokered execution and therefore relies on the immutable
environment policy and release provenance. Read-only filesystem visibility can
still reveal non-secret metadata; deployments needing a narrower view should
mount Soma and provider assets inside a dedicated service sandbox. Component
HTTP uses the same explicit network declarations, public-address validation,
DNS pinning, HTTPS-only policy, response bound, and no-redirect rule.
