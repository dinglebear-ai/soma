use super::super::{
    AgentProjectionOutboxInput, AgentProjectionWriteInput, AgentRunEventUpsert, AgentRunUpsert,
    write_agent_projection,
};
use crate::agent_observatory::{AgentEventKind, RunStatus, StreamEventName};
use crate::{StorageConfig, init_pool};

#[test]
fn projection_write_increments_event_and_error_counters_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let pool = init_pool(&StorageConfig {
        db_path: dir.path().join("counters.db"),
        pool_size: 1,
        wal_mode: false,
        ..StorageConfig::default()
    })
    .unwrap();
    let input = AgentProjectionWriteInput {
        run: AgentRunUpsert {
            native_session_id: "session".into(),
            tool: "Claude".into(),
            provider_tool: None,
            hostname: "dookie".into(),
            parent_run_key: None,
            previous_run_key: None,
            primary_worktree_key: None,
            transcript_path: None,
            process_id: None,
            status: RunStatus::Active,
            status_reason: "test".into(),
            status_observed_at: "2026-08-18T00:00:00Z".into(),
            started_at: "2026-08-18T00:00:00Z".into(),
            last_activity_at: "2026-08-18T00:00:01Z".into(),
            ended_at: None,
            primary_branch: None,
            start_head_sha: None,
            current_head_sha: None,
            projection_version: 1,
            freshness_json: "{}".into(),
            metadata_json: "{}".into(),
        },
        actor: None,
        worktree_evidence: None,
        event: AgentRunEventUpsert {
            source_kind: "ai_logs".into(),
            source_id: "1".into(),
            projection_variant: "error".into(),
            worktree_key: None,
            observed_at: "2026-08-18T00:00:01Z".into(),
            ingested_at: "2026-08-18T00:00:01Z".into(),
            event_kind: AgentEventKind::Error,
            source_log_id: None,
            provider_sequence: None,
            trace_id: None,
            span_id: None,
            severity: "err".into(),
            title: "error".into(),
            summary: "error".into(),
            payload_json: "{}".into(),
            content_scrubbed: true,
        },
        outbox: AgentProjectionOutboxInput {
            event_name: StreamEventName::RunEvent,
            expires_at: "2026-08-19T00:00:00Z".into(),
            payload_json: "{}".into(),
        },
    };
    let result = write_agent_projection(&pool, &input).unwrap();
    assert_eq!(result.run.event_count, 1);
    assert_eq!(result.run.error_count, 1);
    assert_eq!(result.run.last_event_id, Some(result.event.id));
}
