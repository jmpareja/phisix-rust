use crate::db::Db;
use crate::parser::get_default_as_of;
use crate::routes::AppState;
use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

// ── Models ──────────────────────────────────────────────────────────────────

/// A single dividend calendar entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dividend {
    /// Company name
    pub name: String,
    /// Security symbol (e.g. COMMON, BRNPB Series B, ACENB)
    pub symbol: String,
    /// Type of dividend: "Cash", "Stock", "Property", etc.
    pub dividend_type: String,
    /// Raw dividend rate string from PSE (e.g. "Php 2.0625 per share", "P0.50", "0.56%")
    pub dividend_rate: String,
    /// Ex-dividend date (YYYY-MM-DD)
    pub ex_date: String,
    /// Record date (YYYY-MM-DD)
    pub record_date: String,
    /// Payment / payable date (YYYY-MM-DD), may be empty if TBA
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_date: Option<String>,
    /// PSE circular number (e.g. "C00595-2026")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub circular_number: Option<String>,
}

/// Wrapper returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DividendCalendar {
    pub as_of: String,
    pub total: usize,
    pub dividends: Vec<Dividend>,
}

/// Query parameters for filtering dividends.
#[derive(Debug, Deserialize)]
pub struct DividendQuery {
    /// Filter: start date (inclusive, YYYY-MM-DD) for ex_date range
    pub from: Option<String>,
    /// Filter: end date (inclusive, YYYY-MM-DD) for ex_date range
    pub to: Option<String>,
}

// ── Date parsing ────────────────────────────────────────────────────────────

/// Parse PSE date formats like "Aug 04, 2026", "Aug 5, 2026" into "YYYY-MM-DD".
fn parse_pse_date(s: &str) -> Option<String> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Try chrono parsing with two patterns: with and without zero-padded day
    let formats = ["%b %d, %Y", "%b %e, %Y"];
    for fmt in &formats {
        if let Ok(dt) = chrono::NaiveDate::parse_from_str(s, fmt) {
            return Some(dt.format("%Y-%m-%d").to_string());
        }
    }

    warn!("Failed to parse PSE date: '{}'", s);
    None
}

// ── HTML scraping ───────────────────────────────────────────────────────────

const PSE_DIVIDENDS_URL: &str =
    "https://edge.pse.com.ph/disclosureData/dividends_and_rights_info_list.ax";

/// Scrape a single page of dividends from PSE EDGE.
async fn fetch_dividends_page(
    client: &reqwest::Client,
    page: u32,
) -> Result<(Vec<Dividend>, u32), String> {
    let form_body = format!(
        "DividendsOrRights=Dividends&pageNum={}&sortMode=date&dateSortType=DESC&cmpySortType=ASC",
        page
    );

    let response = client
        .post(PSE_DIVIDENDS_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .header(
            "User-Agent",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        )
        .body(form_body)
        .send()
        .await
        .map_err(|e| format!("Request to PSE EDGE failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "PSE EDGE returned status: {}",
            response.status()
        ));
    }

    let html = response
        .text()
        .await
        .map_err(|e| format!("Failed to read PSE EDGE response: {}", e))?;

    parse_dividends_html(&html)
}

/// Parse the HTML fragment returned by the PSE EDGE AJAX endpoint.
///
/// Returns (dividends, total_pages).
fn parse_dividends_html(html: &str) -> Result<(Vec<Dividend>, u32), String> {
    use scraper::{Html, Selector};

    let doc = Html::parse_fragment(html);

    // Extract total pages from "[1 / 11]"
    let count_sel =
        Selector::parse("span.count").map_err(|e| format!("CSS selector error: {:?}", e))?;
    let mut total_pages: u32 = 1;
    if let Some(el) = doc.select(&count_sel).next() {
        let text = el.text().collect::<String>();
        // Pattern: "[1 / 11]"
        if let Some(slash_pos) = text.find('/') {
            let after_slash = &text[slash_pos + 1..];
            // Find the closing bracket
            let end = after_slash.find(']').unwrap_or(after_slash.len());
            if let Ok(pages) = after_slash[..end].trim().parse::<u32>() {
                total_pages = pages;
            }
        }
    }

    // Parse table rows
    let tbody_sel =
        Selector::parse("table.list tbody tr").map_err(|e| format!("CSS selector error: {:?}", e))?;
    let td_sel = Selector::parse("td").map_err(|e| format!("CSS selector error: {:?}", e))?;

    let mut dividends = Vec::new();

    for row in doc.select(&tbody_sel) {
        let cells: Vec<String> = row
            .select(&td_sel)
            .map(|td| {
                td.text()
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
            .collect();

        if cells.len() < 7 {
            continue;
        }

        // columns: 0=Company Name, 1=Type of Security (symbol), 2=Type of Dividend,
        //          3=Dividend Rate, 4=Ex-Dividend Date, 5=Record Date,
        //          6=Payment Date, 7=Circular Number
        let name = cells[0].clone();
        let symbol = cells[1].clone();
        let dividend_type = cells[2].clone();
        let dividend_rate = cells[3].clone();

        let ex_date = match parse_pse_date(&cells[4]) {
            Some(d) => d,
            None => {
                warn!("Skipping dividend row with unparseable ex_date: '{}'", cells[4]);
                continue;
            }
        };

        let record_date = match parse_pse_date(&cells[5]) {
            Some(d) => d,
            None => {
                warn!(
                    "Skipping dividend row with unparseable record_date: '{}'",
                    cells[5]
                );
                continue;
            }
        };

        let payment_date = parse_pse_date(&cells[6]);

        let circular_number = cells.get(7).and_then(|s| {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        });

        dividends.push(Dividend {
            name,
            symbol,
            dividend_type,
            dividend_rate,
            ex_date,
            record_date,
            payment_date,
            circular_number,
        });
    }

    Ok((dividends, total_pages))
}

/// Scrape all pages of dividends from PSE EDGE.
pub async fn scrape_all_dividends(client: &reqwest::Client) -> Result<Vec<Dividend>, String> {
    info!("Scraping PSE EDGE dividends calendar (page 1)...");
    let (mut all_divs, total_pages) = fetch_dividends_page(client, 1).await?;
    info!(
        "Page 1: {} dividends, {} total pages",
        all_divs.len(),
        total_pages
    );

    for page in 2..=total_pages {
        info!("Scraping PSE EDGE dividends calendar (page {}/{})...", page, total_pages);
        match fetch_dividends_page(client, page).await {
            Ok((divs, _)) => {
                info!("Page {}: {} dividends", page, divs.len());
                all_divs.extend(divs);
            }
            Err(e) => {
                warn!("Failed to scrape page {}: {}", page, e);
                // Continue with what we have so far
            }
        }
        // Brief delay to be polite
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    info!(
        "Scraped {} total dividend records from PSE EDGE",
        all_divs.len()
    );
    Ok(all_divs)
}

// ── Database helpers ────────────────────────────────────────────────────────

impl Db {
    /// Create the `dividend` table if it does not exist.
    pub fn init_dividends(&self) -> rusqlite::Result<()> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dividend (
                name            TEXT    NOT NULL,
                symbol          TEXT    NOT NULL,
                dividend_type   TEXT    NOT NULL DEFAULT 'Cash',
                dividend_rate   TEXT    NOT NULL DEFAULT '',
                ex_date         TEXT    NOT NULL,
                record_date     TEXT    NOT NULL,
                payment_date    TEXT,
                circular_number TEXT,
                PRIMARY KEY (symbol, ex_date, dividend_rate)
            )",
            [],
        )?;
        info!("Dividend table initialized");
        Ok(())
    }

    /// Bulk-insert a slice of dividends inside a transaction.
    pub fn save_dividends(&self, divs: &[Dividend]) -> rusqlite::Result<()> {
        let mut conn = self.conn().lock().unwrap();
        let tx = conn.transaction()?;
        for d in divs {
            tx.execute(
                "INSERT OR REPLACE INTO dividend
                    (name, symbol, dividend_type, dividend_rate,
                     ex_date, record_date, payment_date, circular_number)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                rusqlite::params![
                    d.name,
                    d.symbol,
                    d.dividend_type,
                    d.dividend_rate,
                    d.ex_date,
                    d.record_date,
                    d.payment_date,
                    d.circular_number,
                ],
            )?;
        }
        tx.commit()?;
        info!("Saved {} dividend records", divs.len());
        Ok(())
    }

    /// Query dividends, optionally filtered by ex_date range.
    pub fn query_dividends(
        &self,
        from: Option<&str>,
        to: Option<&str>,
    ) -> rusqlite::Result<Vec<Dividend>> {
        let conn = self.conn().lock().unwrap();
        let (sql, params): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = match (from, to) {
            (Some(f), Some(t)) => (
                "SELECT name, symbol, dividend_type, dividend_rate,
                        ex_date, record_date, payment_date, circular_number
                 FROM dividend WHERE ex_date >= ? AND ex_date <= ?
                 ORDER BY ex_date, symbol"
                    .to_string(),
                vec![Box::new(f.to_string()), Box::new(t.to_string())],
            ),
            (Some(f), None) => (
                "SELECT name, symbol, dividend_type, dividend_rate,
                        ex_date, record_date, payment_date, circular_number
                 FROM dividend WHERE ex_date >= ?
                 ORDER BY ex_date, symbol"
                    .to_string(),
                vec![Box::new(f.to_string())],
            ),
            (None, Some(t)) => (
                "SELECT name, symbol, dividend_type, dividend_rate,
                        ex_date, record_date, payment_date, circular_number
                 FROM dividend WHERE ex_date <= ?
                 ORDER BY ex_date, symbol"
                    .to_string(),
                vec![Box::new(t.to_string())],
            ),
            (None, None) => (
                "SELECT name, symbol, dividend_type, dividend_rate,
                        ex_date, record_date, payment_date, circular_number
                 FROM dividend ORDER BY ex_date DESC, symbol
                 LIMIT 200"
                    .to_string(),
                vec![],
            ),
        };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> =
            params.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
            Ok(Dividend {
                name: row.get(0)?,
                symbol: row.get(1)?,
                dividend_type: row.get(2)?,
                dividend_rate: row.get(3)?,
                ex_date: row.get(4)?,
                record_date: row.get(5)?,
                payment_date: row.get(6)?,
                circular_number: row.get(7)?,
            })
        })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    /// Look up dividends for a specific stock symbol.
    pub fn dividends_by_symbol(&self, symbol: &str) -> rusqlite::Result<Vec<Dividend>> {
        let conn = self.conn().lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT name, symbol, dividend_type, dividend_rate,
                    ex_date, record_date, payment_date, circular_number
             FROM dividend WHERE UPPER(symbol) = UPPER(?)
             ORDER BY ex_date DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![symbol], |row| {
            Ok(Dividend {
                name: row.get(0)?,
                symbol: row.get(1)?,
                dividend_type: row.get(2)?,
                dividend_rate: row.get(3)?,
                ex_date: row.get(4)?,
                record_date: row.get(5)?,
                payment_date: row.get(6)?,
                circular_number: row.get(7)?,
            })
        })?;

        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }

    /// Delete a specific dividend by symbol and ex_date.
    pub fn delete_dividend(&self, symbol: &str, ex_date: &str) -> rusqlite::Result<usize> {
        let conn = self.conn().lock().unwrap();
        conn.execute(
            "DELETE FROM dividend WHERE UPPER(symbol) = UPPER(?) AND ex_date = ?",
            rusqlite::params![symbol, ex_date],
        )
    }
}

// ── XML helpers ─────────────────────────────────────────────────────────────

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn dividends_to_xml(cal: &DividendCalendar) -> String {
    let mut xml = String::new();
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    xml.push_str(&format!(
        "<dividends as_of=\"{}\" total=\"{}\">\n",
        cal.as_of, cal.total
    ));
    for d in &cal.dividends {
        xml.push_str(&format!(
            "  <dividend symbol=\"{}\">\n",
            escape_xml(&d.symbol)
        ));
        xml.push_str(&format!(
            "    <name>{}</name>\n",
            escape_xml(&d.name)
        ));
        xml.push_str(&format!(
            "    <dividend_type>{}</dividend_type>\n",
            escape_xml(&d.dividend_type)
        ));
        xml.push_str(&format!(
            "    <dividend_rate>{}</dividend_rate>\n",
            escape_xml(&d.dividend_rate)
        ));
        xml.push_str(&format!("    <ex_date>{}</ex_date>\n", d.ex_date));
        xml.push_str(&format!(
            "    <record_date>{}</record_date>\n",
            d.record_date
        ));
        if let Some(ref pd) = d.payment_date {
            xml.push_str(&format!("    <payment_date>{}</payment_date>\n", pd));
        }
        if let Some(ref cn) = d.circular_number {
            xml.push_str(&format!(
                "    <circular_number>{}</circular_number>\n",
                escape_xml(cn)
            ));
        }
        xml.push_str("  </dividend>\n");
    }
    xml.push_str("</dividends>");
    xml
}

// ── Route handlers ──────────────────────────────────────────────────────────

fn make_dividend_response(cal: &DividendCalendar, xml: bool) -> Response {
    if xml {
        let body = dividends_to_xml(cal);
        (
            StatusCode::OK,
            [(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/xml"),
            )],
            body,
        )
            .into_response()
    } else {
        match serde_json::to_string(cal) {
            Ok(body) => (
                StatusCode::OK,
                [(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                )],
                body,
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("JSON serialization error: {}", e),
            )
                .into_response(),
        }
    }
}

/// GET /dividends
/// GET /dividends.json
///
/// Query params: ?from=YYYY-MM-DD&to=YYYY-MM-DD
pub async fn get_dividends(
    State(state): State<AppState>,
    Query(q): Query<DividendQuery>,
) -> Response {
    match state
        .db
        .query_dividends(q.from.as_deref(), q.to.as_deref())
    {
        Ok(divs) => {
            let total = divs.len();
            let cal = DividendCalendar {
                as_of: get_default_as_of(),
                total,
                dividends: divs,
            };
            make_dividend_response(&cal, false)
        }
        Err(e) => {
            error!("Database error querying dividends: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /dividends.xml
///
/// Same as above but returns XML.
pub async fn get_dividends_xml(
    State(state): State<AppState>,
    Query(q): Query<DividendQuery>,
) -> Response {
    match state
        .db
        .query_dividends(q.from.as_deref(), q.to.as_deref())
    {
        Ok(divs) => {
            let total = divs.len();
            let cal = DividendCalendar {
                as_of: get_default_as_of(),
                total,
                dividends: divs,
            };
            make_dividend_response(&cal, true)
        }
        Err(e) => {
            error!("Database error querying dividends: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET /dividends/:symbol
///
/// Returns all dividend records for a specific stock symbol.
pub async fn get_dividends_by_symbol(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> Response {
    // Strip optional format suffix (.json / .xml)
    let (sym, xml) = if let Some(s) = symbol.strip_suffix(".xml") {
        (s, true)
    } else if let Some(s) = symbol.strip_suffix(".json") {
        (s, false)
    } else {
        (symbol.as_str(), false)
    };

    match state.db.dividends_by_symbol(sym) {
        Ok(divs) => {
            if divs.is_empty() {
                return StatusCode::NOT_FOUND.into_response();
            }
            let total = divs.len();
            let cal = DividendCalendar {
                as_of: get_default_as_of(),
                total,
                dividends: divs,
            };
            make_dividend_response(&cal, xml)
        }
        Err(e) => {
            error!("Database error querying dividends for {}: {}", sym, e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// POST /dividends
///
/// Accepts a JSON body with a single `Dividend` or an array of `Dividend`s.
pub async fn post_dividends(
    State(state): State<AppState>,
    Json(payload): Json<serde_json::Value>,
) -> Response {
    let divs: Vec<Dividend> = if payload.is_array() {
        match serde_json::from_value(payload) {
            Ok(v) => v,
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("Invalid payload: {}", e))
                    .into_response()
            }
        }
    } else {
        match serde_json::from_value::<Dividend>(payload) {
            Ok(d) => vec![d],
            Err(e) => {
                return (StatusCode::BAD_REQUEST, format!("Invalid payload: {}", e))
                    .into_response()
            }
        }
    };

    if divs.is_empty() {
        return (StatusCode::BAD_REQUEST, "Empty dividend list").into_response();
    }

    match state.db.save_dividends(&divs) {
        Ok(_) => {
            info!("Saved {} dividend records via POST", divs.len());
            (
                StatusCode::CREATED,
                format!("Saved {} dividend record(s)", divs.len()),
            )
                .into_response()
        }
        Err(e) => {
            error!("Database error saving dividends: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Database error: {}", e),
            )
                .into_response()
        }
    }
}

/// DELETE /dividends/:symbol/:ex_date
///
/// Deletes a specific dividend record.
pub async fn delete_dividend(
    State(state): State<AppState>,
    Path((symbol, ex_date)): Path<(String, String)>,
) -> Response {
    match state.db.delete_dividend(&symbol, &ex_date) {
        Ok(0) => StatusCode::NOT_FOUND.into_response(),
        Ok(_) => {
            info!("Deleted dividend {} @ {}", symbol, ex_date);
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            error!("Database error deleting dividend: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

/// GET or POST /dividends/scrape
///
/// Scrapes all dividend records from PSE EDGE and stores them in the database.
/// Returns the full scraped calendar as JSON.
pub async fn scrape_dividends(State(state): State<AppState>) -> Response {
    info!("Starting PSE EDGE dividends scrape...");

    match scrape_all_dividends(&state.client).await {
        Ok(divs) => {
            let count = divs.len();

            // Save to database
            if let Err(e) = state.db.save_dividends(&divs) {
                error!("Failed to save scraped dividends to DB: {}", e);
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Scrape succeeded ({} records) but DB save failed: {}", count, e),
                )
                    .into_response();
            }

            let cal = DividendCalendar {
                as_of: get_default_as_of(),
                total: count,
                dividends: divs,
            };

            info!("Scrape complete: {} dividend records saved", count);
            make_dividend_response(&cal, false)
        }
        Err(e) => {
            error!("Dividends scrape failed: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                format!("Scrape failed: {}", e),
            )
                .into_response()
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_pse_date() {
        assert_eq!(parse_pse_date("Aug 04, 2026"), Some("2026-08-04".to_string()));
        assert_eq!(parse_pse_date("Aug 5, 2026"), Some("2026-08-05".to_string()));
        assert_eq!(parse_pse_date("Feb 08, 2027"), Some("2027-02-08".to_string()));
        assert_eq!(parse_pse_date("Dec 01, 2026"), Some("2026-12-01".to_string()));
        assert_eq!(parse_pse_date(""), None);
    }

    #[test]
    fn test_parse_dividends_html() {
        let html = r##"
        <span class="count">[1 / 3][Total 150]</span>
        <table class="list">
        <thead><tr><th>Company Name</th><th>Type of Security</th><th>Type of Dividend</th><th>Dividend Rate</th><th>Ex-Dividend Date</th><th>Record Date</th><th>Payment Date</th><th>Circular Number</th></tr></thead>
        <tbody>
        <tr>
            <td><a href="/companyInformation/form.do?cmpy_id=69">Globe Telecom, Inc.</a></td>
            <td class="alignC">COMMON</td>
            <td class="alignC">Cash</td>
            <td class="alignR">Php25 per common share</td>
            <td class="alignC">Aug 17, 2026</td>
            <td class="alignC">Aug 18, 2026</td>
            <td class="alignC">Sep 3, 2026</td>
            <td class="alignC"><a href="#viewer" onclick="openPopup('abc');return false;">C05886-2026</a></td>
        </tr>
        </tbody>
        </table>
        "##;

        let (divs, total_pages) = parse_dividends_html(html).unwrap();
        assert_eq!(total_pages, 3);
        assert_eq!(divs.len(), 1);

        let d = &divs[0];
        assert_eq!(d.name, "Globe Telecom, Inc.");
        assert_eq!(d.symbol, "COMMON");
        assert_eq!(d.dividend_type, "Cash");
        assert_eq!(d.dividend_rate, "Php25 per common share");
        assert_eq!(d.ex_date, "2026-08-17");
        assert_eq!(d.record_date, "2026-08-18");
        assert_eq!(d.payment_date, Some("2026-09-03".to_string()));
        assert_eq!(d.circular_number, Some("C05886-2026".to_string()));
    }
}
