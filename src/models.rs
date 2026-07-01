use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Price {
    pub currency: String,
    pub amount: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Stock {
    pub name: String,
    pub symbol: String,
    pub price: Price,
    pub percent_change: f64,
    pub volume: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stocks {
    pub as_of: String, // ISO 8601 format: yyyy-MM-dd'T'HH:mm:ss+08:00
    pub stocks: Vec<Stock>,
}
