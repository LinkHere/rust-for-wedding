use axum::{
    routing::{get, post},
    http::Method,
    Router,
    Json,
    extract::State,
    response::IntoResponse,
};
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};
use reqwest::Client;
use yup_oauth2::{ServiceAccountAuthenticator, read_service_account_key};
use std::env;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::error::Error;
use rusqlite::{params, Connection, Result};
use std::sync::Arc;
use tokio::sync::Mutex;
//use reqwest::header::HeaderName;
use chrono::Utc;

#[derive(Debug, Serialize, Deserialize)]
struct SheetData {
    id: Option<i32>,
    name: String,
    value: String,
    timestamp: Option<String>,
}

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok(); // Load environment variables

    let db = Arc::new(Mutex::new(init_db().expect("Failed to initialize database")));

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(vec![Method::GET, Method::POST])
	.allow_headers(Any);
    	//.allow_credentials(true);

    let app = Router::new()
        .route("/sheets", get(fetch_google_sheets))
        .route("/save", post(save_data))
	.route("/data", get(get_saved_data))
        .layer(cors)
        .with_state(AppState { db });

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("Server running on http://127.0.0.1:3000");

    axum::serve(listener, app).await.unwrap();
}

fn init_db() -> Result<Connection> {
    let conn = Connection::open("data.db")?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS sheets (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            value TEXT NOT NULL,
            timestamp TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )",
        [],
    )?;
    Ok(conn)
}

async fn fetch_google_sheets() -> impl IntoResponse {
    match get_google_sheets_data().await {
        Ok(data) => (axum::http::StatusCode::OK, data).into_response(),
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Error: {}", err),
        )
            .into_response(),
    }
}

async fn save_data(
    State(state): State<AppState>,
    Json(mut payload): Json<SheetData>,
) -> impl IntoResponse {
    // Debugging: Print received data
    println!("Received data: {:?}", payload);

    if !payload.value.is_char_boundary(0) {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "Error: Invalid UTF-8 encoding",
        )
            .into_response();
    }

    if payload.value.len() > 300 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            "Error: value exceeds 300 characters",
        )
            .into_response();
    }

    let conn = state.db.lock().await;
    let timestamp = Utc::now().to_rfc3339();
    payload.timestamp = Some(timestamp.clone());

    match conn.execute(
        "INSERT INTO sheets (name, value, timestamp) VALUES (?1, ?2, ?3)",
        params![payload.name, payload.value, timestamp],
    ) {
        Ok(_) => (axum::http::StatusCode::CREATED, "Data saved successfully").into_response(),
        Err(err) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to save data: {}", err),
        )
            .into_response(),
    }
}

async fn get_google_sheets_data() -> Result<String, Box<dyn Error>> {
    let sa_key_path = env::var("SERVICE_ACCOUNT_KEY")?;
    let spreadsheet_id = env::var("SPREADSHEET_ID")?;
    let range = env::var("RANGE")?;

    let sa_key = read_service_account_key(sa_key_path).await?;
    let auth = ServiceAccountAuthenticator::builder(sa_key).build().await?;
    let token = auth.token(&["https://www.googleapis.com/auth/spreadsheets.readonly"]).await?;

    let client = Client::new();
    let url = format!(
        "https://sheets.googleapis.com/v4/spreadsheets/{}/values/{}",
        spreadsheet_id, range
    );

    let response = client
        .get(&url)
        .bearer_auth(token.token().unwrap_or_default())
        .send()
        .await?
        .json::<Value>()
        .await?;

    Ok(response.to_string())
}

async fn get_saved_data(State(state): State<AppState>) -> impl IntoResponse {
    let conn = state.db.lock().await;
    let mut stmt = conn.prepare("SELECT id, name, value, timestamp  FROM sheets").unwrap();

    let rows = stmt
        .query_map([], |row| {
            Ok(SheetData {
                id: row.get(0)?,
                name: row.get(1)?,
                value: row.get(2)?,
		timestamp: row.get(3)?,
            })
        })
        .unwrap();

    let mut results = Vec::new();
    for row in rows {
        results.push(row.unwrap());
    }

    Json(results)
}
