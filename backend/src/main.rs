mod briefing;
mod db;
mod error;
mod gym;
mod seed;
mod settings;
mod tasks;
mod tts;
mod weather;

use axum::routing::{get, post, put};
use axum::Router;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub http: reqwest::Client,
    pub weather_cache: Arc<Mutex<HashMap<String, weather::Weather>>>,
}

fn http_client() -> reqwest::Client {
    let mut b = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30));
    // Optional extra CA bundle (useful behind TLS-intercepting proxies).
    for var in ["PA_CA_BUNDLE", "SSL_CERT_FILE", "REQUESTS_CA_BUNDLE"] {
        if let Ok(path) = std::env::var(var) {
            if let Ok(pem) = std::fs::read(&path) {
                if let Ok(certs) = reqwest::Certificate::from_pem_bundle(&pem) {
                    for c in certs {
                        b = b.add_root_certificate(c);
                    }
                    break;
                }
            }
        }
    }
    b.build().expect("http client")
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/settings", get(settings::get).put(settings::put))
        .route("/api/categories", get(tasks::list_categories).post(tasks::create_category))
        .route("/api/categories/{id}", put(tasks::update_category).delete(tasks::delete_category))
        .route("/api/tasks", get(tasks::list_tasks).post(tasks::create_task))
        .route("/api/tasks/{id}", get(tasks::get_task_h).put(tasks::update_task).delete(tasks::delete_task))
        .route("/api/days", get(tasks::days))
        .route("/api/occurrences/toggle", post(tasks::toggle))
        .route("/api/occurrences/checklist", post(tasks::toggle_checklist))
        .route("/api/exercises", get(gym::list_exercises).post(gym::create_exercise))
        .route("/api/exercises/{id}", put(gym::update_exercise).delete(gym::delete_exercise))
        .route("/api/exercises/{id}/history", get(gym::exercise_history))
        .route("/api/templates", get(gym::list_templates).post(gym::create_template))
        .route("/api/templates/{id}", put(gym::update_template).delete(gym::delete_template))
        .route("/api/rotation", get(gym::rotation_get).put(gym::rotation_put))
        .route("/api/gym/next", get(gym::next))
        .route("/api/gym/rest", post(gym::rest_taken))
        .route("/api/sessions", get(gym::session_list).post(gym::session_create))
        .route("/api/sessions/{id}", get(gym::session_get))
        .route("/api/sessions/{id}/sets", put(gym::session_sets))
        .route("/api/sessions/{id}/complete", post(gym::session_complete))
        .route("/api/briefing", get(briefing::briefing))
        .route("/api/tts", post(tts::speak))
        .route("/health", get(|| async { "ok" }))
        .fallback(error::fallback)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().with_max_level(tracing::Level::INFO).init();
    let path = std::env::var("DB_PATH").unwrap_or_else(|_| "./pa.db".into());
    let conn = db::open(&path).unwrap_or_else(|e| panic!("cannot open database {path}: {e}"));
    seed::seed_if_empty(&conn).expect("seeding failed");
    let state = AppState { db: Arc::new(Mutex::new(conn)), http: http_client(), weather_cache: Arc::default() };
    let port: u16 = std::env::var("PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(8080);
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.unwrap_or_else(|e| panic!("cannot bind port {port}: {e}"));
    tracing::info!("pa-backend listening on 0.0.0.0:{port}, db {path}");
    axum::serve(listener, router(state)).await.unwrap();
}
