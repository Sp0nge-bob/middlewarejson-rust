use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::{
    extract::{Path, RawQuery, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower_http::trace::TraceLayer;
use middlewarejson_core::config::Settings;
use middlewarejson_core::db::{CatalogRepository, Database};
use middlewarejson_core::models::subscription::{validate_payload, validate_sub_id};
use middlewarejson_core::services::panel_api::{resolve_upstream_base_url, PANEL_API_BASE_URL_KEY};
use middlewarejson_core::services::panel_sync::{
    parse_sync_interval, run_panel_sync, MIN_SYNC_INTERVAL,
};
use middlewarejson_core::services::transform_service::TransformService;
use middlewarejson_core::services::upstream::{UpstreamClient, UpstreamClientTrait, UpstreamError};
use serde_json::Value;
use tokio::net::TcpListener;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct AppState {
    settings: Settings,
    transform: Arc<TransformService>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let settings = Settings::from_env();
    log_transform_readiness(&settings)?;

    if settings.panel_sync_on_startup {
        run_panel_sync(&settings, "startup").await;
    }

    if !settings.panel_sync_interval.trim().is_empty() {
        spawn_panel_sync_scheduler(settings.clone());
    }

    let json_path = settings.resolved_agent_json_path();
    let route = format!("{json_path}/{{sub_id}}");

    let state = AppState {
        transform: Arc::new(TransformService::new(settings.clone())?),
        settings: settings.clone(),
    };

    let app = Router::new()
        .route(&route, get(subscription).head(subscription))
        .route("/health", get(health))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = format!("{}:{}", settings.agent_host, settings.agent_port).parse()?;
    info!("listening on http://{addr}{json_path}/<sub_id>");
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    if tokio::signal::ctrl_c().await.is_ok() {
        info!("shutdown signal received");
    }
}

fn log_transform_readiness(settings: &Settings) -> anyhow::Result<()> {
    let repo = CatalogRepository::new(Database::new(&settings.db_path))?;
    let balancer_count = repo.list_balancers()?.len();
    let panel_url_setting = repo.get_setting(PANEL_API_BASE_URL_KEY)?;
    let panel_url = panel_url_setting.as_deref();
    let upstream_base = resolve_upstream_base_url(settings, panel_url);
    let upstream_path = settings.upstream_json_path.trim_end_matches('/');

    info!(
        "TRANSFORM_MODE={}, balancers in db={balancer_count}",
        settings.transform_mode
    );
    info!("upstream target: {upstream_base}{upstream_path}/<sub_id>");

    let web_path = settings.panel_web_base_path.trim().trim_matches('/');
    if !web_path.is_empty() && upstream_base.contains(web_path) {
        warn!(
            "UPSTREAM_BASE_URL looks like panel URL (contains {web_path}). \
             JSON subscriptions are served by a separate 3x-ui sub server."
        );
    }
    if balancer_count > 0 && settings.transform_mode.trim().to_lowercase() != "rules" {
        warn!(
            "balancers exist in db but TRANSFORM_MODE={} — set TRANSFORM_MODE=rules in .env",
            settings.transform_mode
        );
    }
    Ok(())
}

fn spawn_panel_sync_scheduler(settings: Settings) {
    tokio::spawn(async move {
        let interval = match parse_sync_interval(&settings.panel_sync_interval) {
            Ok(interval) => interval,
            Err(error) => {
                tracing::error!("panel sync scheduler disabled: {error}");
                return;
            }
        };
        let Some(interval) = interval else {
            return;
        };
        if interval < MIN_SYNC_INTERVAL {
            tracing::error!(
                "panel sync scheduler disabled: interval {} is below minimum {:?}",
                settings.panel_sync_interval.trim(),
                MIN_SYNC_INTERVAL
            );
            return;
        }

        info!(
            "panel sync scheduler started: every {} ({:.0} sec)",
            settings.panel_sync_interval.trim(),
            interval.as_secs_f64()
        );

        loop {
            tokio::time::sleep(interval).await;
            run_panel_sync(&settings, "schedule").await;
        }
    });
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, r#"{"status":"ok"}"#)
}

async fn subscription(
    State(state): State<AppState>,
    method: Method,
    Path(sub_id): Path<String>,
    RawQuery(query): RawQuery,
    headers: HeaderMap,
) -> Response {
    if !validate_sub_id(&sub_id) {
        return (StatusCode::BAD_REQUEST, "invalid sub_id").into_response();
    }

    let repo = match CatalogRepository::new(Database::new(&state.settings.db_path)) {
        Ok(repo) => repo,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "database error").into_response(),
    };
    let panel_url = repo.get_setting(PANEL_API_BASE_URL_KEY).ok().flatten();
    let upstream_base = resolve_upstream_base_url(&state.settings, panel_url.as_deref());

    let upstream = match UpstreamClient::new(state.settings.clone(), Some(&upstream_base)) {
        Ok(client) => client,
        Err(_) => return (StatusCode::BAD_GATEWAY, "upstream unavailable").into_response(),
    };

    let req_headers: HashMap<String, String> = headers
        .iter()
        .filter_map(|(k, v)| {
            let key = k.as_str().to_ascii_lowercase();
            v.to_str()
                .ok()
                .map(|val| (key, val.to_string()))
        })
        .collect();

    let query_string = query.as_deref().unwrap_or("");
    let result = match upstream
        .fetch(&sub_id, query_string, Some(&req_headers))
        .await
    {
        Ok(result) => result,
        Err(UpstreamError::Timeout | UpstreamError::Unavailable | UpstreamError::Other(_)) => {
            return (StatusCode::BAD_GATEWAY, "upstream unavailable").into_response();
        }
    };

    if result.status_code == 404 {
        return StatusCode::NOT_FOUND.into_response();
    }
    if result.status_code >= 500 {
        return (StatusCode::BAD_GATEWAY, "upstream error").into_response();
    }
    if result.status_code != 200 {
        return (
            StatusCode::from_u16(result.status_code).unwrap_or(StatusCode::BAD_GATEWAY),
            result.body,
        )
            .into_response();
    }

    if method == Method::HEAD {
        let mut resp = Response::new(Default::default());
        *resp.status_mut() = StatusCode::OK;
        apply_upstream_headers(resp.headers_mut(), &result.headers);
        return resp;
    }

    let payload: Value = match serde_json::from_str(&result.body) {
        Ok(value) => value,
        Err(_) => return (StatusCode::BAD_GATEWAY, "invalid upstream json").into_response(),
    };

    if validate_payload(&payload).is_err() {
        return (StatusCode::BAD_GATEWAY, "invalid upstream json").into_response();
    }

    let transformed = match state.transform.transform(&sub_id, &payload) {
        Ok(value) => value,
        Err(error) => {
            warn!("transform failed for sub_id={sub_id}: {error}");
            return (StatusCode::BAD_GATEWAY, "invalid upstream json").into_response();
        }
    };
    let body = serde_json::to_string_pretty(&transformed).unwrap_or_default() + "\n";

    let mut resp = (StatusCode::OK, body).into_response();
    apply_upstream_headers(resp.headers_mut(), &result.headers);
    let response_headers = resp.headers_mut();
    response_headers.insert(
        axum::http::header::CONTENT_TYPE,
        axum::http::HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response_headers.insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("no-store"),
    );
    resp
}

fn apply_upstream_headers(target: &mut axum::http::HeaderMap, headers: &HashMap<String, String>) {
    for (key, value) in headers {
        if let (Ok(name), Ok(header_value)) = (
            axum::http::HeaderName::from_bytes(key.as_bytes()),
            axum::http::HeaderValue::from_str(value),
        ) {
            target.insert(name, header_value);
        }
    }
}