use axum::{
    routing::get,
    http::Method,
    Router,
};
use tokio::net::TcpListener;
use tower_http::cors::{CorsLayer, Any};
use reqwest::Client;
use yup_oauth2::{ServiceAccountAuthenticator, read_service_account_key};
use std::env;
use serde_json::Value;
use std::error::Error;
//use tokio::sync::Mutex;
use axum::response::IntoResponse;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok(); // Load environment variables

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(vec![Method::GET]);

    let app = Router::new()
        .route("/sheets", get(fetch_google_sheets))
        .layer(cors);

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    println!("Server running on http://127.0.0.1:3000");

    axum::serve(listener, app).await.unwrap();
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
