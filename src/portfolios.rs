use crate::db::Db;
use crate::parser::get_default_as_of;
use crate::routes::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

// ── Models ──────────────────────────────────────────────────────────────────

/// An item/holding inside a reference portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioHolding {
    pub symbol: String,
    pub name: String,
    /// Target weight allocation percentage (e.g. 25.0 for 25%)
    pub target_weight_pct: f64,
    /// Sector or asset category (e.g. "Real Estate", "Financials", "Utilities")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sector: Option<String>,
    /// Optional notes or rationale for including this security
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

/// A reference/signature portfolio model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferencePortfolio {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub category: String, // e.g. "Dividend Growth", "High Yield", "Balanced Growth"
    pub risk_level: String, // e.g. "Low", "Moderate", "High"
    pub holdings: Vec<PortfolioHolding>,
    pub created_at: String,
    pub updated_at: String,
}

/// DTO for creating/updating a portfolio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePortfolioPayload {
    pub name: String,
    pub slug: Option<String>,
    pub description: String,
    pub category: String,
    pub risk_level: String,
    pub holdings: Vec<PortfolioHolding>,
}

/// Summary view for portfolio listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioSummary {
    pub id: i64,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub category: String,
    pub risk_level: String,
    pub total_holdings: usize,
    pub updated_at: String,
}

/// API Response Wrapper for Portfolios
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReferencePortfoliosResponse {
    pub as_of: String,
    pub total: usize,
    pub portfolios: Vec<PortfolioSummary>,
}

/// API Response Wrapper for a Single Portfolio Analysis
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioAnalysisHolding {
    pub symbol: String,
    pub name: String,
    pub target_weight_pct: f64,
    pub sector: Option<String>,
    pub live_price: Option<f64>,
    pub live_percent_change: Option<f64>,
    pub live_volume: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortfolioAnalysisResponse {
    pub as_of: String,
    pub portfolio: ReferencePortfolio,
    pub holdings_analysis: Vec<PortfolioAnalysisHolding>,
    pub sector_breakdown: std::collections::HashMap<String, f64>,
}

// ── Database Methods ────────────────────────────────────────────────────────

impl Db {
    /// Initialize tables for reference portfolios.
    pub fn init_portfolios(&self) -> rusqlite::Result<()> {
        let conn = self.conn().lock().unwrap();
        
        conn.execute(
            "CREATE TABLE IF NOT EXISTS reference_portfolio (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT    NOT NULL,
                slug        TEXT    NOT NULL UNIQUE,
                description TEXT    NOT NULL,
                category    TEXT    NOT NULL,
                risk_level  TEXT    NOT NULL,
                created_at  TEXT    NOT NULL,
                updated_at  TEXT    NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS portfolio_holding (
                id                  INTEGER PRIMARY KEY AUTOINCREMENT,
                portfolio_id        INTEGER NOT NULL,
                symbol              TEXT    NOT NULL,
                name                TEXT    NOT NULL,
                target_weight_pct   REAL    NOT NULL,
                sector              TEXT,
                notes               TEXT,
                FOREIGN KEY (portfolio_id) REFERENCES reference_portfolio (id) ON DELETE CASCADE
            )",
            [],
        )?;

        info!("Reference portfolios tables initialized successfully");
        Ok(())
    }

    /// Seed default reference portfolios if missing.
    pub fn seed_default_portfolios(&self) -> rusqlite::Result<()> {
        info!("Checking default reference portfolios...");
        
        let defaults = vec![
            CreatePortfolioPayload {
                name: "DragonFi Dividend Benchmark (D15)".to_string(),
                slug: Some("dragonfi-d15".to_string()),
                description: "DragonFi's carefully selected list of 15 high-quality dividend-paying stocks with strong earnings, reliable payouts, and clear potential for long-term dividend growth.".to_string(),
                category: "Dividend Benchmark".to_string(),
                risk_level: "Moderate".to_string(),
                holdings: vec![
                    PortfolioHolding { symbol: "AP".to_string(), name: "ABOITIZ POWER CORPORATION".to_string(), target_weight_pct: 7.62, sector: Some("Industrials".to_string()), notes: Some("Yield 3.14% (TTM) | Buy below ₱44.00".to_string()) },
                    PortfolioHolding { symbol: "AREIT".to_string(), name: "AREIT INC.".to_string(), target_weight_pct: 6.64, sector: Some("Property".to_string()), notes: Some("Yield 6.53% (TTM) | Buy below ₱39.50".to_string()) },
                    PortfolioHolding { symbol: "BPI".to_string(), name: "BANK OF THE PHILIPPINE ISLANDS".to_string(), target_weight_pct: 4.58, sector: Some("Financials".to_string()), notes: Some("Yield 4.70% (TTM) | Buy below ₱120.00".to_string()) },
                    PortfolioHolding { symbol: "CBC".to_string(), name: "CHINA BANKING CORPORATION".to_string(), target_weight_pct: 5.58, sector: Some("Financials".to_string()), notes: Some("Yield 3.43% (TTM) | Buy below ₱70.00".to_string()) },
                    PortfolioHolding { symbol: "CREIT".to_string(), name: "CITICORE ENERGY REIT CORP.".to_string(), target_weight_pct: 6.02, sector: Some("Property".to_string()), notes: Some("Yield 5.97% (TTM) | Buy below ₱3.50".to_string()) },
                    PortfolioHolding { symbol: "ICT".to_string(), name: "INTERNATIONAL CONTAINER TERMINAL SERVICES INC.".to_string(), target_weight_pct: 17.80, sector: Some("Services".to_string()), notes: Some("Yield 1.78% (TTM) | Buy below ₱630.00".to_string()) },
                    PortfolioHolding { symbol: "KEEPR".to_string(), name: "THE KEEPERS HOLDINGS INC.".to_string(), target_weight_pct: 3.35, sector: Some("Industrials".to_string()), notes: Some("Yield 6.35% (TTM) | Buy below ₱2.58".to_string()) },
                    PortfolioHolding { symbol: "LTG".to_string(), name: "LT GROUP INC.".to_string(), target_weight_pct: 7.88, sector: Some("Holding Firms".to_string()), notes: Some("Yield 1.01% (TTM) | Buy below ₱15.00".to_string()) },
                    PortfolioHolding { symbol: "MBT".to_string(), name: "METROPOLITAN BANK & TRUST COMPANY".to_string(), target_weight_pct: 5.90, sector: Some("Financials".to_string()), notes: Some("Yield 4.50% (TTM) | Buy below ₱70.00".to_string()) },
                    PortfolioHolding { symbol: "MER".to_string(), name: "MANILA ELECTRIC COMPANY".to_string(), target_weight_pct: 4.27, sector: Some("Industrials".to_string()), notes: Some("Yield 5.90% (TTM) | Buy below ₱580.00".to_string()) },
                    PortfolioHolding { symbol: "MWC".to_string(), name: "MANILA WATER COMPANY INC.".to_string(), target_weight_pct: 3.03, sector: Some("Industrials".to_string()), notes: Some("Yield 5.18% (TTM) | Buy below ₱39.00".to_string()) },
                    PortfolioHolding { symbol: "MYNLD".to_string(), name: "MAYNILAD WATER SERVICES INC.".to_string(), target_weight_pct: 1.67, sector: Some("Industrials".to_string()), notes: Some("Yield 6.03% (TTM) | Buy below ₱19.30".to_string()) },
                    PortfolioHolding { symbol: "OGP".to_string(), name: "OCEANAGOLD (PHILIPPINES) INC.".to_string(), target_weight_pct: 12.56, sector: Some("Mining & Oil".to_string()), notes: Some("Yield 10.35% (TTM) | Buy below ₱32.00".to_string()) },
                    PortfolioHolding { symbol: "RCR".to_string(), name: "RL COMMERCIAL REIT INC.".to_string(), target_weight_pct: 7.70, sector: Some("Property".to_string()), notes: Some("Yield 5.99% (TTM) | Buy below ₱7.75".to_string()) },
                    PortfolioHolding { symbol: "TEL".to_string(), name: "PLDT INC.".to_string(), target_weight_pct: 5.40, sector: Some("Services".to_string()), notes: Some("Yield 7.71% (TTM) | Buy below ₱1,350.00".to_string()) },
                ],
            },
            CreatePortfolioPayload {
                name: "InvestingPH Signature Portfolio".to_string(),
                slug: Some("investingph-signature".to_string()),
                description: "InvestingPH's portfolio applying his dividend value strategy to invest in strong, dividend-paying stocks for steady, stress-free income for the long term.".to_string(),
                category: "Dividend Value".to_string(),
                risk_level: "Moderate".to_string(),
                holdings: vec![
                    PortfolioHolding { symbol: "AREIT".to_string(), name: "AREIT INC.".to_string(), target_weight_pct: 6.92, sector: Some("Property".to_string()), notes: Some("Yield 6.53% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "CREIT".to_string(), name: "CITICORE ENERGY REIT CORP.".to_string(), target_weight_pct: 6.92, sector: Some("Property".to_string()), notes: Some("Yield 5.97% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "DMC".to_string(), name: "DMCI HOLDINGS INC.".to_string(), target_weight_pct: 3.85, sector: Some("Mining & Oil".to_string()), notes: Some("Yield 4.07% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "DNL".to_string(), name: "D&L INDUSTRIES INC.".to_string(), target_weight_pct: 7.69, sector: Some("Industrials".to_string()), notes: Some("Yield 5.10% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "FILRT".to_string(), name: "FILINVEST REIT CORP.".to_string(), target_weight_pct: 6.92, sector: Some("Property".to_string()), notes: Some("Yield 8.06% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "FLI".to_string(), name: "FILINVEST LAND INC.".to_string(), target_weight_pct: 6.92, sector: Some("Property".to_string()), notes: Some("Yield 7.14% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "GLO".to_string(), name: "GLOBE TELECOM INC.".to_string(), target_weight_pct: 5.77, sector: Some("Services".to_string()), notes: Some("Yield 5.70% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "KEEPR".to_string(), name: "THE KEEPERS HOLDINGS INC.".to_string(), target_weight_pct: 7.69, sector: Some("Industrials".to_string()), notes: Some("Yield 6.35% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "LTG".to_string(), name: "LT GROUP INC.".to_string(), target_weight_pct: 11.54, sector: Some("Holding Firms".to_string()), notes: Some("Yield 1.01% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "MBT".to_string(), name: "METROPOLITAN BANK & TRUST COMPANY".to_string(), target_weight_pct: 7.69, sector: Some("Financials".to_string()), notes: Some("Yield 4.50% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "MEG".to_string(), name: "MEGAWORLD CORPORATION".to_string(), target_weight_pct: 6.92, sector: Some("Property".to_string()), notes: Some("Yield 4.21% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "MER".to_string(), name: "MANILA ELECTRIC COMPANY".to_string(), target_weight_pct: 7.69, sector: Some("Industrials".to_string()), notes: Some("Yield 5.90% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "RFM".to_string(), name: "RFM CORPORATION".to_string(), target_weight_pct: 0.0, sector: Some("Industrials".to_string()), notes: Some("Yield 7.81% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "SECB".to_string(), name: "SECURITY BANK CORPORATION".to_string(), target_weight_pct: 7.69, sector: Some("Financials".to_string()), notes: Some("Yield 4.41% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "TEL".to_string(), name: "PLDT INC.".to_string(), target_weight_pct: 5.77, sector: Some("Services".to_string()), notes: Some("Yield 7.71% (TTM)".to_string()) },
                    PortfolioHolding { symbol: "MYNLD".to_string(), name: "MAYNILAD WATER SERVICES INC.".to_string(), target_weight_pct: 0.0, sector: Some("Industrials".to_string()), notes: Some("Yield 6.03% (TTM)".to_string()) },
                ],
            },
            CreatePortfolioPayload {
                name: "Dividend Harvest Portfolio".to_string(),
                slug: Some("dividend-harvest".to_string()),
                description: "Investing Club's Dividend Harvest Portfolio investing in high-quality dividend-paying stocks that offer safe, consistent, and growing dividends.".to_string(),
                category: "Dividend Income".to_string(),
                risk_level: "Moderate".to_string(),
                holdings: vec![
                    PortfolioHolding { symbol: "ALI".to_string(), name: "AYALA LAND INC.".to_string(), target_weight_pct: 3.25, sector: Some("Property".to_string()), notes: Some("Yield 4.02% (TTM) | Buy below ₱19.00".to_string()) },
                    PortfolioHolding { symbol: "AP".to_string(), name: "ABOITIZ POWER CORPORATION".to_string(), target_weight_pct: 9.18, sector: Some("Industrials".to_string()), notes: Some("Yield 3.14% (TTM) | Buy below ₱47.00".to_string()) },
                    PortfolioHolding { symbol: "AREIT".to_string(), name: "AREIT INC.".to_string(), target_weight_pct: 8.00, sector: Some("Property".to_string()), notes: Some("Yield 6.53% (TTM) | Buy below ₱39.50".to_string()) },
                    PortfolioHolding { symbol: "CREIT".to_string(), name: "CITICORE ENERGY REIT CORP.".to_string(), target_weight_pct: 7.26, sector: Some("Property".to_string()), notes: Some("Yield 5.97% (TTM) | Buy below ₱3.35".to_string()) },
                    PortfolioHolding { symbol: "GLO".to_string(), name: "GLOBE TELECOM INC.".to_string(), target_weight_pct: 18.72, sector: Some("Services".to_string()), notes: Some("Yield 5.70% (TTM) | Buy below ₱1,650.00".to_string()) },
                    PortfolioHolding { symbol: "ICT".to_string(), name: "INTERNATIONAL CONTAINER TERMINAL SERVICES INC.".to_string(), target_weight_pct: 21.45, sector: Some("Services".to_string()), notes: Some("Yield 1.78% (TTM) | Buy below ₱670.00".to_string()) },
                    PortfolioHolding { symbol: "MBT".to_string(), name: "METROPOLITAN BANK & TRUST COMPANY".to_string(), target_weight_pct: 1.42, sector: Some("Financials".to_string()), notes: Some("Yield 4.50% (TTM) | Buy below ₱71.50".to_string()) },
                    PortfolioHolding { symbol: "MER".to_string(), name: "MANILA ELECTRIC COMPANY".to_string(), target_weight_pct: 10.29, sector: Some("Industrials".to_string()), notes: Some("Yield 5.90% (TTM) | Buy below ₱580.00".to_string()) },
                    PortfolioHolding { symbol: "MWC".to_string(), name: "MANILA WATER COMPANY INC.".to_string(), target_weight_pct: 7.30, sector: Some("Industrials".to_string()), notes: Some("Yield 5.18% (TTM) | Buy below ₱39.00".to_string()) },
                    PortfolioHolding { symbol: "OGP".to_string(), name: "OCEANAGOLD (PHILIPPINES) INC.".to_string(), target_weight_pct: 7.56, sector: Some("Mining & Oil".to_string()), notes: Some("Yield 10.35% (TTM) | Buy below ₱35.00".to_string()) },
                    PortfolioHolding { symbol: "RCR".to_string(), name: "RL COMMERCIAL REIT INC.".to_string(), target_weight_pct: 1.54, sector: Some("Property".to_string()), notes: Some("Yield 5.99% (TTM) | Buy below ₱7.20".to_string()) },
                    PortfolioHolding { symbol: "MYNLD".to_string(), name: "MAYNILAD WATER SERVICES INC.".to_string(), target_weight_pct: 4.03, sector: Some("Industrials".to_string()), notes: Some("Yield 6.03% (TTM) | Buy below ₱20.00".to_string()) },
                ],
            },
            CreatePortfolioPayload {
                name: "eTrader Managed Signature Portfolio".to_string(),
                slug: Some("etrader-signature".to_string()),
                description: "Actively managed portfolio by eTrader (16+ years in PH equities) targeting capital growth and medium-term investing across key PSE sectors.".to_string(),
                category: "Capital Growth".to_string(),
                risk_level: "Aggressive".to_string(),
                holdings: vec![
                    PortfolioHolding { symbol: "AC".to_string(), name: "Ayala Corporation".to_string(), target_weight_pct: 17.64, sector: Some("Holding Firms".to_string()), notes: Some("Buy below ₱480.00".to_string()) },
                    PortfolioHolding { symbol: "ALI".to_string(), name: "Ayala Land, Inc.".to_string(), target_weight_pct: 5.41, sector: Some("Property".to_string()), notes: Some("Buy below ₱21.00".to_string()) },
                    PortfolioHolding { symbol: "FGEN".to_string(), name: "First Gen Corporation".to_string(), target_weight_pct: 6.47, sector: Some("Industrials".to_string()), notes: Some("Buy below ₱20.10".to_string()) },
                    PortfolioHolding { symbol: "GTCAP".to_string(), name: "GT Capital Holdings, Inc.".to_string(), target_weight_pct: 17.53, sector: Some("Holding Firms".to_string()), notes: Some("Buy below ₱580.00".to_string()) },
                    PortfolioHolding { symbol: "JGS".to_string(), name: "JG Summit Holdings, Inc.".to_string(), target_weight_pct: 7.99, sector: Some("Holding Firms".to_string()), notes: Some("Buy below ₱23.00".to_string()) },
                    PortfolioHolding { symbol: "MWC".to_string(), name: "Manila Water Company, Inc.".to_string(), target_weight_pct: 12.14, sector: Some("Industrials".to_string()), notes: Some("Buy below ₱38.00".to_string()) },
                    PortfolioHolding { symbol: "PNB".to_string(), name: "Philippine National Bank".to_string(), target_weight_pct: 22.18, sector: Some("Financials".to_string()), notes: Some("Buy below ₱52.00".to_string()) },
                    PortfolioHolding { symbol: "ACEN".to_string(), name: "ACEN Corporation".to_string(), target_weight_pct: 10.65, sector: Some("Industrials".to_string()), notes: Some("Buy below ₱2.60".to_string()) },
                ],
            },
            CreatePortfolioPayload {
                name: "DragonFi High Yield Dividend Portfolio".to_string(),
                slug: Some("dragonfi-high-yield".to_string()),
                description: "A signature reference portfolio focusing on top PSE dividend-paying REITs and blue-chip equities with high yield and cashflow stability.".to_string(),
                category: "Dividend Yield".to_string(),
                risk_level: "Moderate".to_string(),
                holdings: vec![
                    PortfolioHolding { symbol: "AREIT".to_string(), name: "AREIT, Inc.".to_string(), target_weight_pct: 20.0, sector: Some("Real Estate / REIT".to_string()), notes: Some("Anchor REIT stock".to_string()) },
                    PortfolioHolding { symbol: "MREIT".to_string(), name: "MREIT, Inc.".to_string(), target_weight_pct: 15.0, sector: Some("Real Estate / REIT".to_string()), notes: None },
                    PortfolioHolding { symbol: "RCR".to_string(), name: "RL Commercial REIT, Inc.".to_string(), target_weight_pct: 15.0, sector: Some("Real Estate / REIT".to_string()), notes: None },
                    PortfolioHolding { symbol: "TEL".to_string(), name: "PLDT Inc.".to_string(), target_weight_pct: 15.0, sector: Some("Telecommunications".to_string()), notes: Some("High dividend payout yield".to_string()) },
                    PortfolioHolding { symbol: "GLO".to_string(), name: "Globe Telecom, Inc.".to_string(), target_weight_pct: 15.0, sector: Some("Telecommunications".to_string()), notes: None },
                    PortfolioHolding { symbol: "MER".to_string(), name: "Manila Electric Company".to_string(), target_weight_pct: 20.0, sector: Some("Utilities".to_string()), notes: Some("Consistent dividend distribution".to_string()) },
                ],
            },
            CreatePortfolioPayload {
                name: "PSEi Core Growth Portfolio".to_string(),
                slug: Some("psei-core-growth".to_string()),
                description: "Balanced reference allocation tracking premier Philippine conglomerates and market leaders for capital appreciation.".to_string(),
                category: "Balanced Growth".to_string(),
                risk_level: "Moderate".to_string(),
                holdings: vec![
                    PortfolioHolding { symbol: "SM".to_string(), name: "SM Investments Corporation".to_string(), target_weight_pct: 25.0, sector: Some("Holding Firms".to_string()), notes: None },
                    PortfolioHolding { symbol: "ALI".to_string(), name: "Ayala Land, Inc.".to_string(), target_weight_pct: 20.0, sector: Some("Real Estate".to_string()), notes: None },
                    PortfolioHolding { symbol: "BDO".to_string(), name: "BDO Unibank, Inc.".to_string(), target_weight_pct: 20.0, sector: Some("Financials".to_string()), notes: None },
                    PortfolioHolding { symbol: "BPI".to_string(), name: "Bank of the Philippine Islands".to_string(), target_weight_pct: 15.0, sector: Some("Financials".to_string()), notes: None },
                    PortfolioHolding { symbol: "JFC".to_string(), name: "Jollibee Foods Corporation".to_string(), target_weight_pct: 20.0, sector: Some("Consumer".to_string()), notes: None },
                ],
            },
        ];

        for p in defaults {
            if let Some(ref slug) = p.slug {
                if self.get_portfolio(slug)?.is_none() {
                    self.save_portfolio(&p)?;
                }
            }
        }

        Ok(())
    }

    /// Save (create) a reference portfolio.
    pub fn save_portfolio(&self, payload: &CreatePortfolioPayload) -> rusqlite::Result<i64> {
        let now = get_default_as_of();
        let slug = payload.slug.clone().unwrap_or_else(|| {
            payload.name.to_lowercase().replace(' ', "-").replace(|c: char| !c.is_alphanumeric() && c != '-', "")
        });

        let mut conn = self.conn().lock().unwrap();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO reference_portfolio (name, slug, description, category, risk_level, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            rusqlite::params![payload.name, slug, payload.description, payload.category, payload.risk_level, now, now],
        )?;

        let portfolio_id = tx.last_insert_rowid();

        for h in &payload.holdings {
            tx.execute(
                "INSERT INTO portfolio_holding (portfolio_id, symbol, name, target_weight_pct, sector, notes)
                 VALUES (?, ?, ?, ?, ?, ?)",
                rusqlite::params![portfolio_id, h.symbol.to_ascii_uppercase(), h.name, h.target_weight_pct, h.sector, h.notes],
            )?;
        }

        tx.commit()?;
        info!("Saved reference portfolio '{}' with ID {}", payload.name, portfolio_id);
        Ok(portfolio_id)
    }

    /// List all reference portfolios.
    pub fn list_portfolios(&self) -> rusqlite::Result<Vec<PortfolioSummary>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT p.id, p.name, p.slug, p.description, p.category, p.risk_level, p.updated_at, COUNT(h.id)
             FROM reference_portfolio p
             LEFT JOIN portfolio_holding h ON p.id = h.portfolio_id
             GROUP BY p.id
             ORDER BY p.id ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(PortfolioSummary {
                id: row.get(0)?,
                name: row.get(1)?,
                slug: row.get(2)?,
                description: row.get(3)?,
                category: row.get(4)?,
                risk_level: row.get(5)?,
                updated_at: row.get(6)?,
                total_holdings: row.get(7)?,
            })
        })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    /// Get a single reference portfolio by ID or Slug.
    pub fn get_portfolio(&self, id_or_slug: &str) -> rusqlite::Result<Option<ReferencePortfolio>> {
        let conn = self.conn().lock().unwrap();
        
        let portfolio_row = if let Ok(id) = id_or_slug.parse::<i64>() {
            conn.query_row(
                "SELECT id, name, slug, description, category, risk_level, created_at, updated_at
                 FROM reference_portfolio WHERE id = ?",
                rusqlite::params![id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
            )
        } else {
            conn.query_row(
                "SELECT id, name, slug, description, category, risk_level, created_at, updated_at
                 FROM reference_portfolio WHERE LOWER(slug) = LOWER(?)",
                rusqlite::params![id_or_slug],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
            )
        };

        let (id, name, slug, description, category, risk_level, created_at, updated_at): (i64, String, String, String, String, String, String, String) = match portfolio_row {
            Ok(data) => data,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
            Err(e) => return Err(e),
        };

        let mut h_stmt = conn.prepare(
            "SELECT symbol, name, target_weight_pct, sector, notes
             FROM portfolio_holding WHERE portfolio_id = ? ORDER BY target_weight_pct DESC",
        )?;

        let h_rows = h_stmt.query_map(rusqlite::params![id], |r| {
            Ok(PortfolioHolding {
                symbol: r.get(0)?,
                name: r.get(1)?,
                target_weight_pct: r.get(2)?,
                sector: r.get(3)?,
                notes: r.get(4)?,
            })
        })?;

        let mut holdings = Vec::new();
        for hr in h_rows {
            holdings.push(hr?);
        }

        Ok(Some(ReferencePortfolio {
            id,
            name,
            slug,
            description,
            category,
            risk_level,
            holdings,
            created_at,
            updated_at,
        }))
    }

    /// Delete a reference portfolio by ID or Slug.
    pub fn delete_portfolio(&self, id_or_slug: &str) -> rusqlite::Result<bool> {
        let conn = self.conn().lock().unwrap();
        let count = if let Ok(id) = id_or_slug.parse::<i64>() {
            conn.execute("DELETE FROM reference_portfolio WHERE id = ?", rusqlite::params![id])?
        } else {
            conn.execute("DELETE FROM reference_portfolio WHERE LOWER(slug) = LOWER(?)", rusqlite::params![id_or_slug])?
        };
        Ok(count > 0)
    }
}

// ── Route Handlers ──────────────────────────────────────────────────────────

/// GET /portfolios
/// List all reference portfolios.
pub async fn list_reference_portfolios(State(state): State<AppState>) -> Response {
    match state.db.list_portfolios() {
        Ok(list) => {
            let total = list.len();
            let res = ReferencePortfoliosResponse {
                as_of: get_default_as_of(),
                total,
                portfolios: list,
            };
            (StatusCode::OK, Json(res)).into_response()
        }
        Err(e) => {
            error!("Error listing reference portfolios: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /portfolios/:id_or_slug
/// Fetch full details for a reference portfolio.
pub async fn get_reference_portfolio(
    State(state): State<AppState>,
    Path(id_or_slug): Path<String>,
) -> Response {
    match state.db.get_portfolio(&id_or_slug) {
        Ok(Some(p)) => (StatusCode::OK, Json(p)).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("Error getting portfolio {}: {}", id_or_slug, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /portfolios/:id_or_slug/analysis
/// Get enriched analysis of a reference portfolio combined with live market stock data.
pub async fn get_portfolio_analysis(
    State(state): State<AppState>,
    Path(id_or_slug): Path<String>,
) -> Response {
    let portfolio = match state.db.get_portfolio(&id_or_slug) {
        Ok(Some(p)) => p,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("Error getting portfolio {}: {}", id_or_slug, e);
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    // Try fetching live stock market prices to enrich holdings
    let live_stocks_map: std::collections::HashMap<String, crate::models::Stock> = match crate::scraper::fetch_pse_stocks().await {
        Ok(stocks) => stocks.stocks.into_iter().map(|s| (s.symbol.to_ascii_uppercase(), s)).collect(),
        Err(e) => {
            error!("Could not fetch live market prices for analysis enrichment: {}", e);
            std::collections::HashMap::new()
        }
    };

    let mut holdings_analysis = Vec::new();
    let mut sector_breakdown: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for h in &portfolio.holdings {
        let sym_upper = h.symbol.to_ascii_uppercase();
        let live = live_stocks_map.get(&sym_upper);

        let sector_name = h.sector.clone().unwrap_or_else(|| "Other / Uncategorized".to_string());
        *sector_breakdown.entry(sector_name).or_insert(0.0) += h.target_weight_pct;

        holdings_analysis.push(PortfolioAnalysisHolding {
            symbol: h.symbol.clone(),
            name: h.name.clone(),
            target_weight_pct: h.target_weight_pct,
            sector: h.sector.clone(),
            live_price: live.map(|s| s.price.amount),
            live_percent_change: live.map(|s| s.percent_change),
            live_volume: live.map(|s| s.volume),
        });
    }

    let response = PortfolioAnalysisResponse {
        as_of: get_default_as_of(),
        portfolio,
        holdings_analysis,
        sector_breakdown,
    };

    (StatusCode::OK, Json(response)).into_response()
}

/// POST /portfolios
/// Create a new reference portfolio.
pub async fn create_reference_portfolio(
    State(state): State<AppState>,
    Json(payload): Json<CreatePortfolioPayload>,
) -> Response {
    if payload.name.trim().is_empty() || payload.holdings.is_empty() {
        return (StatusCode::BAD_REQUEST, "Portfolio name and holdings are required").into_response();
    }

    match state.db.save_portfolio(&payload) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "message": "Portfolio created successfully",
                "id": id
            })),
        )
            .into_response(),
        Err(e) => {
            error!("Database error creating portfolio: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, format!("Database error: {}", e)).into_response()
        }
    }
}

/// DELETE /portfolios/:id_or_slug
/// Delete a reference portfolio.
pub async fn delete_reference_portfolio(
    State(state): State<AppState>,
    Path(id_or_slug): Path<String>,
) -> Response {
    match state.db.delete_portfolio(&id_or_slug) {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => {
            error!("Database error deleting portfolio {}: {}", id_or_slug, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}
