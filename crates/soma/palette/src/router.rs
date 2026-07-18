//! `/v1/palette/*` route wiring.
//!
//! Handlers are thin: parse the HTTP input, call into `catalog`/`search`/
//! `schema`/`execute`, and translate the result into a response. All product
//! logic lives in those sibling modules, not here.

use axum::{
    extract::{rejection::JsonRejection, Extension, Query, State},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use soma_application::CatalogSnapshot;
use soma_http_api::response::json_rejection_response;

use crate::{
    auth::{palette_execution_context, AuthContext},
    catalog::catalog_response,
    dto::{
        LauncherExecuteRequest, LauncherSchemaQuery, LauncherSearchQuery, LauncherSearchResponse,
    },
    error::{launcher_not_found, palette_error_response},
    execute::{execute_launcher, ExecuteOutcome},
    schema::find_schema,
    search::search_entries,
    state::PaletteState,
};

pub fn router() -> Router<PaletteState> {
    Router::new()
        .route("/v1/palette/catalog", get(get_catalog))
        .route("/v1/palette/search", get(get_search))
        .route("/v1/palette/schema", get(get_schema))
        .route("/v1/palette/execute", post(post_execute))
}

/// Refresh the file-backed provider registry before taking a catalog
/// snapshot. Every catalog-dependent `/v1/palette/*` handler goes through
/// this instead of `catalog_snapshot()` directly — REST (`/v1/providers`)
/// and MCP already refresh before serving, and reading a snapshot straight
/// off the live registry without it left palette responses stale until an
/// unrelated endpoint (or a process restart) happened to refresh it first.
async fn refreshed_snapshot(state: &PaletteState) -> Result<CatalogSnapshot, Response> {
    state
        .application()
        .refresh_providers()
        .map_err(palette_error_response)
}

async fn get_catalog(State(state): State<PaletteState>) -> Response {
    let snapshot = match refreshed_snapshot(&state).await {
        Ok(snapshot) => snapshot,
        Err(response) => return response,
    };
    Json(catalog_response(&snapshot)).into_response()
}

async fn get_search(
    State(state): State<PaletteState>,
    Query(query): Query<LauncherSearchQuery>,
) -> Response {
    let snapshot = match refreshed_snapshot(&state).await {
        Ok(snapshot) => snapshot,
        Err(response) => return response,
    };
    let entries = crate::catalog::palette_entries(&snapshot);
    let results = search_entries(&entries, &query.q, query.limit);
    Json(LauncherSearchResponse { entries: results }).into_response()
}

async fn get_schema(
    State(state): State<PaletteState>,
    Query(query): Query<LauncherSchemaQuery>,
) -> Response {
    let snapshot = match refreshed_snapshot(&state).await {
        Ok(snapshot) => snapshot,
        Err(response) => return response,
    };
    match find_schema(&snapshot, &query.id) {
        Some(schema) => Json(schema).into_response(),
        None => launcher_not_found(&query.id),
    }
}

async fn post_execute(
    State(state): State<PaletteState>,
    auth: Option<Extension<AuthContext>>,
    body: Result<Json<LauncherExecuteRequest>, JsonRejection>,
) -> Response {
    let Json(request) = match body {
        Ok(body) => body,
        // Delegate to `soma-http-api`'s shared rejection renderer — same
        // 413/400 split and `ErrorBody` shape `soma-api` uses for every
        // `JsonRejection` (see `soma_api::responses::rest_json_rejection_response`),
        // instead of a palette-local, always-400, differently-shaped body.
        Err(error) => return json_rejection_response(error),
    };
    let context = palette_execution_context(&state, auth.as_ref().map(|Extension(auth)| auth));
    let id = request.id.clone();

    match execute_launcher(&state, request, context).await {
        ExecuteOutcome::Ok(response) => Json(response).into_response(),
        ExecuteOutcome::NotFound => launcher_not_found(&id),
        ExecuteOutcome::Failed(error) => palette_error_response(error),
    }
}

#[cfg(test)]
#[path = "router_tests.rs"]
mod tests;
