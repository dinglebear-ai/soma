use super::*;
use crate::agent_observatory::RunStatus;

#[test]
fn enum_value_maps_valid_and_invalid_database_text() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let status = conn
        .query_row("SELECT 'active'", [], |row| {
            enum_value::<RunStatus>(row, 0, "status")
        })
        .unwrap();
    assert_eq!(status, RunStatus::Active);
    let invalid = conn.query_row("SELECT 'not-a-status'", [], |row| {
        enum_value::<RunStatus>(row, 0, "status")
    });
    assert!(matches!(
        invalid,
        Err(rusqlite::Error::FromSqlConversionFailure(..))
    ));
}
