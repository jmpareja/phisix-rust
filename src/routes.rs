use crate::models::Stocks;
use crate::db::Db;
use crate::scraper::fetch_pse_stocks;
use crate::parser::parse_stocks_json;
use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn, error};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub cache: Arc<tokio::sync::RwLock<Option<(Instant, Stocks)>>>,
    pub client: reqwest::Client,
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResponseFormat {
    Json,
    Xml,
}

pub struct RequestParams {
    pub symbol: String,
    pub date: Option<String>, // "YYYY-MM-DD"
    pub format: ResponseFormat,
}

// Custom XML escape function
fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&apos;")
}

// Convert Stocks struct to custom JAXB-compatible XML format
pub fn stocks_to_xml(stocks: &Stocks) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<stocks:stocks as_of=\"{}\" xmlns:stocks=\"http://phisix-api.appspot.com/phisix-stocks\">\n", 
        stocks.as_of
    ));
    for stock in &stocks.stocks {
        xml.push_str(&format!("  <stocks:stock symbol=\"{}\">\n", escape_xml(&stock.symbol)));
        xml.push_str(&format!("    <stocks:name>{}</stocks:name>\n", escape_xml(&stock.name)));
        xml.push_str("    <stocks:price>\n");
        xml.push_str(&format!("      <stocks:currency>{}</stocks:currency>\n", escape_xml(&stock.price.currency)));
        xml.push_str(&format!("      <stocks:amount>{}</stocks:amount>\n", stock.price.amount));
        xml.push_str("    </stocks:price>\n");
        xml.push_str(&format!("    <stocks:percent_change>{}</stocks:percent_change>\n", stock.percent_change));
        xml.push_str(&format!("    <stocks:volume>{}</stocks:volume>\n", stock.volume));
        xml.push_str("  </stocks:stock>\n");
    }
    xml.push_str("</stocks:stocks>");
    xml
}

// Custom response builder supporting JSON and XML
pub fn make_response(stocks: &Stocks, format: ResponseFormat) -> Response {
    match format {
        ResponseFormat::Json => {
            let body = match serde_json::to_string(stocks) {
                Ok(s) => s,
                Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, format!("JSON serialization error: {}", e)).into_response(),
            };
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, HeaderValue::from_static("application/json"))],
                body,
            ).into_response()
        }
        ResponseFormat::Xml => {
            let body = stocks_to_xml(stocks);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, HeaderValue::from_static("application/xml"))],
                body,
            ).into_response()
        }
    }
}

pub fn parse_path_params(param: &str) -> Option<RequestParams> {
    let parts: Vec<&str> = param.split('.').collect();
    if parts.is_empty() {
        return None;
    }
    
    let mut format = ResponseFormat::Json;
    let mut date: Option<String> = None;
    let symbol = parts[0].to_string();
    
    let mut remaining_parts = &parts[1..];
    if let Some(&last) = parts.last() {
        if last.to_lowercase() == "json" {
            format = ResponseFormat::Json;
            remaining_parts = &parts[1..parts.len() - 1];
        } else if last.to_lowercase() == "xml" {
            format = ResponseFormat::Xml;
            remaining_parts = &parts[1..parts.len() - 1];
        }
    }
    
    if let Some(&date_str) = remaining_parts.first() {
        // Simple validation for YYYY-MM-DD
        if date_str.len() == 10 
            && date_str.as_bytes()[4] == b'-' 
            && date_str.as_bytes()[7] == b'-' 
        {
            date = Some(date_str.to_string());
        }
    }
    
    Some(RequestParams {
        symbol,
        date,
        format,
    })
}

// Fetch all stocks (with 60-second caching)
async fn get_all_stocks_internal(state: &AppState) -> Result<Stocks, StatusCode> {
    // Check cache
    {
        let cache = state.cache.read().await;
        if let Some((instant, stocks)) = &*cache {
            if instant.elapsed() < Duration::from_secs(60) {
                return Ok(stocks.clone());
            }
        }
    }

    // Cache miss: scrape PSE
    match fetch_pse_stocks().await {
        Ok(stocks) => {
            let mut cache = state.cache.write().await;
            *cache = Some((Instant::now(), stocks.clone()));
            Ok(stocks)
        }
        Err(err) => {
            error!("Error fetching stocks: {}", err);
            // Check if we have expired cache we can serve as fallback
            let cache = state.cache.read().await;
            if let Some((_, stocks)) = &*cache {
                warn!("Serving expired stocks cache due to scraper error");
                Ok(stocks.clone())
            } else {
                Err(StatusCode::SERVICE_UNAVAILABLE)
            }
        }
    }
}

// GET /stocks
// GET /stocks.json
pub async fn get_stocks_json(State(state): State<AppState>) -> Response {
    match get_all_stocks_internal(&state).await {
        Ok(stocks) => make_response(&stocks, ResponseFormat::Json),
        Err(status) => status.into_response(),
    }
}

// GET /stocks.xml
pub async fn get_stocks_xml(State(state): State<AppState>) -> Response {
    match get_all_stocks_internal(&state).await {
        Ok(stocks) => make_response(&stocks, ResponseFormat::Xml),
        Err(status) => status.into_response(),
    }
}

// GET /stocks/:param
pub async fn get_stocks_by_param(
    State(state): State<AppState>,
    Path(param): Path<String>,
) -> Response {
    let request_params = match parse_path_params(&param) {
        Some(p) => p,
        None => return StatusCode::BAD_REQUEST.into_response(),
    };

    if let Some(date_str) = request_params.date {
        // Query historical date
        // 1. Check SQLite
        match state.db.find_by_symbol_and_trading_date(&request_params.symbol, &date_str) {
            Ok(Some(stocks)) => make_response(&stocks, request_params.format),
            Ok(None) => {
                // 2. Local DB miss: Fallback proxy to external API
                // https://phisix-api2.appspot.com/stocks/{symbol}.{date}.json
                let proxy_url = format!(
                    "https://phisix-api2.appspot.com/stocks/{}.{}.json",
                    request_params.symbol.to_ascii_uppercase(),
                    date_str
                );
                info!("Historical stock not found in DB. Proxying to {}", proxy_url);
                match state.client.get(&proxy_url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        if let Ok(body) = resp.text().await {
                            if let Some(stocks) = parse_stocks_json(&body) {
                                // Save to local DB for caching
                                if let Err(e) = state.db.save(&stocks) {
                                    error!("Failed to save proxied stock history to DB: {}", e);
                                }
                                make_response(&stocks, request_params.format)
                            } else {
                                StatusCode::NOT_FOUND.into_response()
                            }
                        } else {
                            StatusCode::NOT_FOUND.into_response()
                        }
                    }
                    _ => StatusCode::NOT_FOUND.into_response(),
                }
            }
            Err(e) => {
                error!("Database error: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR.into_response()
            }
        }
    } else {
        // Query live single stock symbol
        match get_all_stocks_internal(&state).await {
            Ok(all_stocks) => {
                let target_symbol = request_params.symbol.to_ascii_uppercase();
                let found_stock = all_stocks.stocks.into_iter().find(|s| s.symbol.to_ascii_uppercase() == target_symbol);
                if let Some(stock) = found_stock {
                    let single_stock = Stocks {
                        as_of: all_stocks.as_of.clone(),
                        stocks: vec![stock],
                    };
                    make_response(&single_stock, request_params.format)
                } else {
                    StatusCode::NOT_FOUND.into_response()
                }
            }
            Err(status) => status.into_response(),
        }
    }
}

// GET or POST /stocks/archive
pub async fn archive_stocks(State(state): State<AppState>) -> Response {
    info!("Archiving current stock list to database...");
    match get_all_stocks_internal(&state).await {
        Ok(stocks) => {
            match state.db.save(&stocks) {
                Ok(_) => (StatusCode::OK, "Archived successfully").into_response(),
                Err(e) => {
                    error!("Database save error during archiving: {}", e);
                    (StatusCode::INTERNAL_SERVER_ERROR, format!("Database save error: {}", e)).into_response()
                }
            }
        }
        Err(status) => (status, "Failed to fetch live stock data for archiving").into_response(),
    }
}
