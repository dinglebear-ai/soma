# Definition of Ready

A vertical slice may begin only when:

```text
[ ] Donor commits are pinned
[ ] Donor paths are mapped
[ ] Existing behavior and tests are inventoried
[ ] Overlap decisions are resolved
[ ] Shared/product boundary is accepted
[ ] Input/output contracts are drafted
[ ] Security and redaction constraints are known
[ ] Canonical and derived storage authority is explicit
[ ] Migration/rebuild impact is understood
[ ] Acceptance scenario is defined
[ ] Required external services/fixtures are available
[ ] Non-goals are explicit
[ ] PR train is scoped
```

A crate implementation may begin only when:

```text
[ ] Purpose and exclusions are accepted
[ ] Dependency layer is assigned
[ ] Public type ownership is clear
[ ] Feature/default policy is drafted
[ ] Donor parity fixtures are selected
[ ] Independent consumer scenario is defined
[ ] `soma-<one-word>` naming is applied and crates.io availability is checked
```

Ready does not require every implementation detail to be known. It does require the team to know what success means and what must not leak into the boundary.
