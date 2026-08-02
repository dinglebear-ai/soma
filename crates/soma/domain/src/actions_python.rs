use super::{ActionCost, ActionSpec, ActionTransport, CliSpec, ParamSpec, READ_SCOPE, WRITE_SCOPE};

const PRUNE_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "stale_before_unix_seconds",
        ty: "integer",
        required: true,
        description: "Only select non-ready cache entries last modified at or before this Unix timestamp.",
        max_len: None,
        enum_values: &[],
    },
    ParamSpec {
        name: "max_entries",
        ty: "integer",
        required: false,
        description: "Maximum cache entries to inspect in this bounded operation (default 100, maximum 1000).",
        max_len: None,
        enum_values: &[],
    },
];

const PRUNE_APPLY_PARAMS: &[ParamSpec] = &[
    PRUNE_PARAMS[0],
    ParamSpec {
        name: "max_entries",
        ty: "integer",
        required: false,
        description: "Maximum cache entries to remove in this bounded operation (default 100, maximum 1000).",
        max_len: None,
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to apply the destructive prune plan.",
        max_len: None,
        enum_values: &[],
    },
];

const PROVIDER_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "provider_path",
        ty: "string",
        required: true,
        description: "Managed Python provider path, relative to the configured provider directory or absolute within it.",
        max_len: Some(4096),
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to mutate the provider environment lifecycle.",
        max_len: None,
        enum_values: &[],
    },
];

const WORKER_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "provider",
        ty: "string",
        required: true,
        description: "Loaded persistent Python provider name.",
        max_len: Some(256),
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to interrupt or reset worker state.",
        max_len: None,
        enum_values: &[],
    },
];

const ROLLBACK_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "generation_id",
        ty: "integer",
        required: true,
        description: "Retained generation identifier to reactivate.",
        max_len: None,
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to replace the active generation.",
        max_len: None,
        enum_values: &[],
    },
];

const GRADUATION_STATUS_PARAMS: &[ParamSpec] = &[ParamSpec {
    name: "workspace",
    ty: "string",
    required: true,
    description: "Absolute graduation workspace path.",
    max_len: Some(4096),
    enum_values: &[],
}];

const GRADUATION_APPLY_PARAMS: &[ParamSpec] = &[
    ParamSpec {
        name: "operation",
        ty: "string",
        required: true,
        description: "Graduation operation to perform.",
        max_len: Some(32),
        enum_values: &[
            "graduate",
            "build-component",
            "verify-component",
            "compare",
            "componentize-scan",
            "componentize-bindings",
            "componentize-build",
            "componentize-validate",
            "activate",
            "rollback",
        ],
    },
    ParamSpec {
        name: "workspace",
        ty: "string",
        required: true,
        description: "Absolute graduation workspace path.",
        max_len: Some(4096),
        enum_values: &[],
    },
    ParamSpec {
        name: "source",
        ty: "string",
        required: false,
        description: "Python source path required by the graduate operation.",
        max_len: Some(4096),
        enum_values: &[],
    },
    ParamSpec {
        name: "component",
        ty: "string",
        required: false,
        description: "Component path used by build, verify, and compare.",
        max_len: Some(4096),
        enum_values: &[],
    },
    ParamSpec {
        name: "fixtures",
        ty: "string",
        required: false,
        description: "Conformance fixture path used by graduate and compare.",
        max_len: Some(4096),
        enum_values: &[],
    },
    ParamSpec {
        name: "wheelhouse",
        ty: "string",
        required: false,
        description: "Directory containing authenticated pure-Python dependency wheels for componentize-py.",
        max_len: Some(4096),
        enum_values: &[],
    },
    ParamSpec {
        name: "confirm",
        ty: "boolean",
        required: true,
        description: "Must be true to mutate graduation or live provider state.",
        max_len: None,
        enum_values: &[],
    },
];

const fn cli(
    command: &'static str,
    usage: &'static str,
    description: &'static str,
) -> Option<CliSpec> {
    Some(CliSpec {
        command,
        usage,
        flags: &[],
        description,
    })
}

pub(super) const ENVIRONMENT_STATUS: ActionSpec = ActionSpec {
    name: "python_environment_status",
    description: "Inspect immutable Python environment cache state without executing provider code.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("GET"),
    rest_path: Some("/v1/python/environments"),
    destructive: false,
    requires_admin: true,
    cost: ActionCost::Cheap,
    params: &[],
    returns: "PythonEnvironmentStatus",
    cli: cli(
        "python_environment_status",
        "soma python_environment_status",
        "Inspect immutable Python environment cache state.",
    ),
};

pub(super) const ENVIRONMENT_PRUNE_PLAN: ActionSpec = ActionSpec {
    name: "python_environment_prune_plan",
    description: "Plan a bounded prune of stale non-ready Python environment cache entries.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/environments/prune-plan"),
    destructive: false,
    requires_admin: true,
    cost: ActionCost::Moderate,
    params: PRUNE_PARAMS,
    returns: "PythonEnvironmentPrunePlan",
    cli: cli(
        "python_environment_prune_plan",
        "soma python_environment_prune_plan --json '{\"stale_before_unix_seconds\": 0}'",
        "Preview a bounded cache prune without mutation.",
    ),
};

pub(super) const ENVIRONMENT_PRUNE: ActionSpec = ActionSpec {
    name: "python_environment_prune",
    description: "Apply a bounded prune of stale non-ready Python environment cache entries.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/environments/prune"),
    destructive: true,
    requires_admin: false,
    cost: ActionCost::Write,
    params: PRUNE_APPLY_PARAMS,
    returns: "PythonEnvironmentPruneReport",
    cli: cli(
        "python_environment_prune",
        "soma python_environment_prune --json '{\"stale_before_unix_seconds\": 0, \"confirm\": true}'",
        "Apply a confirmed bounded cache prune.",
    ),
};

pub(super) const ENVIRONMENT_REPAIR: ActionSpec = ActionSpec {
    name: "python_environment_repair",
    description: "Repair the immutable environment for one managed Python provider.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/environments/repair"),
    destructive: true,
    requires_admin: false,
    cost: ActionCost::Write,
    params: PROVIDER_PARAMS,
    returns: "PythonEnvironmentRepairReport",
    cli: cli(
        "python_environment_repair",
        "soma python_environment_repair --json '{\"provider_path\": \"example.py\", \"confirm\": true}'",
        "Repair one managed provider environment.",
    ),
};

pub(super) const ENVIRONMENT_UPDATE: ActionSpec = ActionSpec {
    name: "python_environment_update",
    description: "Resolve, prepare, validate, and atomically activate an immutable update for one managed Python provider.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/environments/update"),
    destructive: true,
    requires_admin: false,
    cost: ActionCost::Write,
    params: PROVIDER_PARAMS,
    returns: "PythonEnvironmentUpdateReport",
    cli: cli(
        "python_environment_update",
        "soma python_environment_update --json '{\"provider_path\": \"example.py\", \"confirm\": true}'",
        "Prepare and atomically activate one provider update.",
    ),
};

pub(super) const WORKER_STATUS: ActionSpec = ActionSpec {
    name: "python_worker_status",
    description: "Inspect persistent Python worker health, quarantine, restart counts, and bounded redacted logs.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("GET"),
    rest_path: Some("/v1/python/workers"),
    destructive: false,
    requires_admin: true,
    cost: ActionCost::Cheap,
    params: &[],
    returns: "PythonWorkerStatus",
    cli: cli(
        "python_worker_status",
        "soma python_worker_status",
        "Inspect persistent Python worker state and logs.",
    ),
};

pub(super) const WORKER_CANCEL: ActionSpec = ActionSpec {
    name: "python_worker_cancel",
    description: "Cancel one active persistent Python invocation by terminating its process tree.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/workers/cancel"),
    destructive: true,
    requires_admin: false,
    cost: ActionCost::Write,
    params: WORKER_PARAMS,
    returns: "PythonWorkerCancellation",
    cli: cli(
        "python_worker_cancel",
        "soma python_worker_cancel --json '{\"provider\": \"example\", \"confirm\": true}'",
        "Cancel an active persistent Python invocation.",
    ),
};

pub(super) const WORKER_RESET: ActionSpec = ActionSpec {
    name: "python_worker_reset",
    description: "Clear one persistent Python worker crash-loop quarantine.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/workers/reset"),
    destructive: true,
    requires_admin: false,
    cost: ActionCost::Write,
    params: WORKER_PARAMS,
    returns: "PythonWorkerReset",
    cli: cli(
        "python_worker_reset",
        "soma python_worker_reset --json '{\"provider\": \"example\", \"confirm\": true}'",
        "Clear a persistent worker quarantine.",
    ),
};

pub(super) const GENERATION_STATUS: ActionSpec = ActionSpec {
    name: "python_generation_status",
    description: "Inspect the active Python provider generation and bounded rollback history.",
    required_scope: Some(READ_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("GET"),
    rest_path: Some("/v1/python/generations"),
    destructive: false,
    requires_admin: false,
    cost: ActionCost::Cheap,
    params: &[],
    returns: "PythonGenerationStatus",
    cli: cli(
        "python_generation_status",
        "soma python_generation_status",
        "Inspect active and retained Python provider generations.",
    ),
};

pub(super) const GENERATION_ROLLBACK: ActionSpec = ActionSpec {
    name: "python_generation_rollback",
    description: "Atomically reactivate a retained Python provider generation.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/generations/rollback"),
    destructive: true,
    requires_admin: false,
    cost: ActionCost::Write,
    params: ROLLBACK_PARAMS,
    returns: "PythonGenerationRollback",
    cli: cli(
        "python_generation_rollback",
        "soma python_generation_rollback --json '{\"generation_id\": 1, \"confirm\": true}'",
        "Atomically reactivate a retained generation.",
    ),
};

pub(super) const GRADUATION_STATUS: ActionSpec = ActionSpec {
    name: "python_graduation_status",
    description: "Inspect digest-bound Python graduation, conformance, activation, and rollback state.",
    required_scope: Some(READ_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/graduation/status"),
    destructive: false,
    requires_admin: true,
    cost: ActionCost::Cheap,
    params: GRADUATION_STATUS_PARAMS,
    returns: "PythonGraduationStatus",
    cli: cli(
        "python_graduation_status",
        "soma python_graduation_status --json '{\"workspace\":\"/absolute/path\"}'",
        "Inspect a graduation workspace.",
    ),
};

pub(super) const GRADUATION_APPLY: ActionSpec = ActionSpec {
    name: "python_graduation_apply",
    description: "Scaffold, componentize, build, verify, compare, activate, or roll back a Python graduation workspace.",
    required_scope: Some(WRITE_SCOPE),
    transport: ActionTransport::Any,
    rest_method: Some("POST"),
    rest_path: Some("/v1/python/graduation/apply"),
    destructive: true,
    requires_admin: true,
    cost: ActionCost::Write,
    params: GRADUATION_APPLY_PARAMS,
    returns: "PythonGraduationReport",
    cli: cli(
        "python_graduation_apply",
        "soma python_graduation_apply --json '{\"operation\":\"compare\",\"workspace\":\"/absolute/path\",\"fixtures\":\"/absolute/fixtures.json\",\"confirm\":true}'",
        "Run a confirmed graduation operation.",
    ),
};
