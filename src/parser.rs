use crate::models::{Stock, Price, Stocks};
use serde_json::Value;
use chrono::{DateTime, Utc, TimeZone, NaiveDateTime, FixedOffset};

fn get_manila_offset() -> FixedOffset {
    FixedOffset::east_opt(8 * 3600).unwrap()
}

pub fn get_default_as_of() -> String {
    let offset = get_manila_offset();
    let now = Utc::now().with_timezone(&offset);
    // Zero out hours, minutes, seconds, milliseconds
    let now_naive = now.date_naive().and_hms_opt(0, 0, 0).unwrap();
    let dt = offset.from_local_datetime(&now_naive).unwrap();
    dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string()
}

pub fn parse_as_of_string(as_of_str: &str) -> Option<String> {
    let offset = get_manila_offset();
    // Try ISO 8601 formats
    // format yyyy-MM-dd'T'HH:mm:ssXXX
    if let Ok(dt) = DateTime::parse_from_rfc3339(as_of_str) {
        let local_dt = dt.with_timezone(&offset);
        return Some(local_dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string());
    }
    // Try parsing without timezone offsets (e.g. yyyy-MM-dd'T'HH:mm:ss)
    if let Ok(ndt) = NaiveDateTime::parse_from_str(as_of_str, "%Y-%m-%dT%H:%M:%S") {
        if let Some(local_dt) = offset.from_local_datetime(&ndt).latest() {
            return Some(local_dt.format("%Y-%m-%dT%H:%M:%S%:z").to_string());
        }
    }
    None
}

fn parse_volume(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Some(i)
            } else {
                n.as_f64().map(|f| f as i64)
            }
        }
        Value::String(s) => {
            let clean = s.replace(",", "");
            if clean.is_empty() {
                return None;
            }
            if let Ok(i) = clean.parse::<i64>() {
                Some(i)
            } else if let Ok(f) = clean.parse::<f64>() {
                Some(f as i64)
            } else {
                None
            }
        }
        _ => None,
    }
}

fn parse_price_amount(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => {
            let clean = s.replace(",", "");
            clean.parse::<f64>().ok()
        }
        _ => None,
    }
}

fn parse_percent_change(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => {
            if s == "null" {
                0.0
            } else {
                s.parse::<f64>().unwrap_or(0.0)
            }
        }
        _ => 0.0,
    }
}

pub fn parse_raw_stock(raw: &Value) -> Option<Stock> {
    let obj = raw.as_object()?;
    
    // Extract stock name
    let name = obj.get("StockName")
        .or_else(|| obj.get("name"))
        .and_then(|v| v.as_str())?
        .to_string();
        
    // Extract stock symbol
    let symbol = obj.get("StockSymbol")
        .or_else(|| obj.get("symbol"))
        .and_then(|v| v.as_str())?
        .to_string();
        
    // Extract volume
    let volume_val = obj.get("Volume").or_else(|| obj.get("volume"))?;
    let volume = parse_volume(volume_val)?;
    
    // Extract percent change
    let pct_change_val = obj.get("PercentChange").or_else(|| obj.get("percent_change"));
    let percent_change = pct_change_val.map(parse_percent_change).unwrap_or(0.0);
    
    // Extract price
    let mut currency = "PHP".to_string();
    let mut amount = 0.0;
    
    if let Some(price_val) = obj.get("Price") {
        if let Some(amt) = parse_price_amount(price_val) {
            amount = amt;
        }
    } else if let Some(price_obj_val) = obj.get("price") {
        if let Some(price_obj) = price_obj_val.as_object() {
            if let Some(curr_val) = price_obj.get("currency") {
                if let Some(curr_str) = curr_val.as_str() {
                    currency = curr_str.to_string();
                }
            }
            if let Some(amount_val) = price_obj.get("amount") {
                if let Some(amt) = parse_price_amount(amount_val) {
                    amount = amt;
                }
            }
        }
    }
    
    let symbol = if name.to_ascii_uppercase() == "PSEI" {
        "PSEi".to_string()
    } else {
        symbol
    };
    
    Some(Stock {
        name,
        symbol,
        price: Price { currency, amount },
        percent_change,
        volume,
    })
}

pub fn parse_stocks_json(json_str: &str) -> Option<Stocks> {
    let value: Value = serde_json::from_str(json_str).ok()?;
    
    let mut stocks_list = Vec::new();
    let mut as_of = get_default_as_of();
    
    if value.is_array() {
        if let Some(arr) = value.as_array() {
            for item in arr {
                if let Some(stock) = parse_raw_stock(item) {
                    stocks_list.push(stock);
                }
            }
        }
    } else if value.is_object() {
        if let Some(obj) = value.as_object() {
            // Extract as_of
            if let Some(as_of_val) = obj.get("as_of").and_then(|v| v.as_str()) {
                if let Some(parsed) = parse_as_of_string(as_of_val) {
                    as_of = parsed;
                }
            }
            
            // Extract stocks
            let stocks_array = obj.get("stock")
                .or_else(|| obj.get("stocks"))
                .and_then(|v| v.as_array());
                
            if let Some(arr) = stocks_array {
                for item in arr {
                    if let Some(stock) = parse_raw_stock(item) {
                        stocks_list.push(stock);
                    }
                }
            } else {
                // Fallback: treat object as single stock if it parses
                if let Some(stock) = parse_raw_stock(&value) {
                    stocks_list.push(stock);
                }
            }
        }
    }
    
    Some(Stocks {
        as_of,
        stocks: stocks_list,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_parse_sample() {
        let content = fs::read_to_string("../phisix/src/test/resources/JsonRawSample.txt")
            .expect("Failed to read JsonRawSample.txt");
        let stocks = parse_stocks_json(&content).expect("Failed to parse JSON");
        assert_eq!(stocks.stocks.len(), 363);
        
        let alco = stocks.stocks.iter().find(|s| s.symbol == "ALCO").expect("ALCO not found");
        assert_eq!(alco.name, "Arthaland Corporation");
        assert_eq!(alco.price.currency, "PHP");
        assert_eq!(alco.price.amount, 0.495);
        assert_eq!(alco.percent_change, 3.13);
        assert_eq!(alco.volume, 10000);

        let psec = stocks.stocks.iter().find(|s| s.symbol == "PSE").expect("PSE not found");
        assert_eq!(psec.name, "The Philippine Stock Exchange, Inc.");
    }

    #[test]
    fn test_psei_override() {
        let raw_json = r#"{
            "Volume": "5,552.34",
            "indicator": "U",
            "PercentChange": "38.97",
            "Price": "0.71",
            "StockName": "PSEi",
            "StockSymbol": "PSE"
        }"#;
        let val: Value = serde_json::from_str(raw_json).unwrap();
        let stock = parse_raw_stock(&val).expect("Failed to parse raw stock");
        assert_eq!(stock.name, "PSEi");
        assert_eq!(stock.symbol, "PSEi");
        assert_eq!(stock.volume, 5552);
        assert_eq!(stock.price.amount, 0.71);
        assert_eq!(stock.percent_change, 38.97);
    }
}
