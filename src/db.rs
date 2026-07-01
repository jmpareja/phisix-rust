use crate::models::{Stock, Price, Stocks};
use rusqlite::{params, Connection, Result};
use std::sync::{Arc, Mutex};
use tracing::{info, error};

#[derive(Clone)]
pub struct Db {
    conn: Arc<Mutex<Connection>>,
}

impl Db {
    pub fn open(path: &str) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Db {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init()?;
        Ok(db)
    }

    pub fn init(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        
        // Create stock table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS stock (
                symbol TEXT PRIMARY KEY,
                name TEXT NOT NULL
            )",
            [],
        )?;

        // Create history table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS history (
                symbol TEXT NOT NULL,
                trading_date TEXT NOT NULL,
                close_price TEXT NOT NULL,
                volume INTEGER NOT NULL,
                PRIMARY KEY (symbol, trading_date),
                FOREIGN KEY (symbol) REFERENCES stock (symbol)
            )",
            [],
        )?;

        info!("Database initialized successfully");
        Ok(())
    }

    pub fn save(&self, stocks: &Stocks) -> Result<()> {
        // Extract YYYY-MM-DD from as_of (e.g. "2025-10-31T00:00:00+08:00")
        let trading_date = if stocks.as_of.len() >= 10 {
            &stocks.as_of[..10]
        } else {
            error!("Invalid as_of date: {}, cannot archive", stocks.as_of);
            return Err(rusqlite::Error::InvalidQuery);
        };

        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        for stock in &stocks.stocks {
            // Save stock metadata (insert or ignore)
            tx.execute(
                "INSERT OR IGNORE INTO stock (symbol, name) VALUES (?, ?)",
                params![stock.symbol, stock.name],
            )?;

            // Save history record (insert or replace)
            let close_str = stock.price.amount.to_string();
            tx.execute(
                "INSERT OR REPLACE INTO history (symbol, trading_date, close_price, volume) 
                 VALUES (?, ?, ?, ?)",
                params![stock.symbol, trading_date, close_str, stock.volume],
            )?;
        }

        tx.commit()?;
        info!("Archived {} stocks for date {}", stocks.stocks.len(), trading_date);
        Ok(())
    }

    pub fn find_by_symbol_and_trading_date(&self, symbol: &str, trading_date: &str) -> Result<Option<Stocks>> {
        // symbol: look up case-insensitively
        // trading_date: "YYYY-MM-DD"
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT s.name, h.close_price, h.volume 
             FROM history h 
             JOIN stock s ON h.symbol = s.symbol 
             WHERE UPPER(h.symbol) = UPPER(?) AND h.trading_date = ?"
        )?;

        let mut rows = stmt.query(params![symbol, trading_date])?;

        if let Some(row) = rows.next()? {
            let name: String = row.get(0)?;
            let close_str: String = row.get(1)?;
            let volume: i64 = row.get(2)?;

            let amount: f64 = close_str.parse().unwrap_or(0.0);

            let stock = Stock {
                name,
                symbol: symbol.to_ascii_uppercase(),
                price: Price {
                    currency: "PHP".to_string(),
                    amount,
                },
                percent_change: 0.0, // History does not store percent change
                volume,
            };

            // Format as_of date: {trading_date}T00:00:00+08:00
            let as_of = format!("{}T00:00:00+08:00", trading_date);

            Ok(Some(Stocks {
                as_of,
                stocks: vec![stock],
            }))
        } else {
            Ok(None)
        }
    }
}
