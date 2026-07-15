use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::cell::RefCell;
use std::io::{self, BufReader, BufWriter};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodeModeRunnerInput {
    Start {
        code: String,
        #[serde(default)]
        proxy: String,
    },
    ToolResult {
        seq: u64,
        result: Value,
    },
    SnippetResolved {
        seq: u64,
        code: String,
        input: Value,
    },
    ToolError {
        seq: u64,
        kind: String,
        message: String,
    },
    StepDecision {
        seq: u64,
        #[serde(default)]
        replay: Option<Value>,
    },
    StepRecorded {
        seq: u64,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CodeModeRunnerOutput {
    ToolCall {
        seq: u64,
        id: String,
        params: Value,
    },
    ArtifactWrite {
        seq: u64,
        path: String,
        content: String,
        #[serde(default)]
        content_type: Option<String>,
    },
    SnippetResolve {
        seq: u64,
        name: String,
        #[serde(default)]
        input: Value,
    },
    StepBegin {
        seq: u64,
        name: String,
    },
    StepResult {
        seq: u64,
        value: Value,
    },
    Done {
        #[serde(default)]
        result: CodeModeRunnerResult,
        #[serde(default)]
        logs: Vec<String>,
    },
    Error {
        kind: String,
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(tag = "state", content = "value", rename_all = "snake_case")]
pub enum CodeModeRunnerResult {
    #[default]
    Undefined,
    Json(Value),
}

impl CodeModeRunnerResult {
    #[must_use]
    pub fn from_response_result(result: Option<Value>) -> Self {
        result.map_or(Self::Undefined, Self::Json)
    }

    #[must_use]
    pub fn into_response_result(self) -> Option<Value> {
        match self {
            Self::Undefined => None,
            Self::Json(value) => Some(value),
        }
    }
}

pub const CODE_MODE_STACK_SIZE_LIMIT: usize = 256 * 1024;

pub(crate) struct CodeModeRunnerState {
    pub reader: BufReader<io::Stdin>,
    pub writer: BufWriter<io::Stdout>,
    pub next_seq: u64,
}

thread_local! {
    pub(crate) static RUNNER_STATE: RefCell<Option<CodeModeRunnerState>> =
        const { RefCell::new(None) };
}
