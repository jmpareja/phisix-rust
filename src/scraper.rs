use crate::models::Stocks;
use crate::parser::parse_stocks_json;
use scraper::{Html, Selector};
use std::time::Duration;
use tracing::info;

pub async fn fetch_pse_stocks() -> Result<Stocks, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| format!("Failed to build reqwest client: {}", e))?;

    info!("Connecting to https://frames.pse.com.ph...");
    let response = client.get("https://frames.pse.com.ph")
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Server returned error status: {}", response.status()));
    }

    let html_content = response.text().await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let document = Html::parse_document(&html_content);
    let selector = Selector::parse("#JsonId")
        .map_err(|e| format!("Failed to parse CSS selector: {:?}", e))?;

    let input_element = document.select(&selector).next()
        .ok_or_else(|| "Could not find element with id 'JsonId' in the response HTML".to_string())?;

    let json_value = input_element.value().attr("value")
        .ok_or_else(|| "Element with id 'JsonId' does not have a 'value' attribute".to_string())?;

    let stocks = parse_stocks_json(json_value)
        .ok_or_else(|| "Failed to parse stocks from the JSON value in the HTML".to_string())?;

    info!("Successfully fetched and parsed {} stocks", stocks.stocks.len());
    Ok(stocks)
}
