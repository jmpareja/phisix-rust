# PHISIX API (Rust Port)

A high-performance Rust port of the Philippine Stock Exchange Composite Index (PSEi) RESTful API, originally written in Java/Maven for Google AppEngine.

This application connects to `https://frames.pse.com.ph` to scrape and parse live stock information, serving it as JSON or JAXB-compatible XML. It also supports local database archiving using SQLite and automatic fallback proxying for historical stock queries.

## Features

- **Live Stocks**: `GET /stocks` (defaults to JSON)
  - Also matches suffix formats: `/stocks.json` and `/stocks.xml`
- **Single Stock lookup**: `GET /stocks/{symbol}` (e.g. `/stocks/ali`, `/stocks/ali.json`, `/stocks/ali.xml`)
- **Historical Stock lookup**: `GET /stocks/{symbol}.{date}` (e.g. `/stocks/ali.2023-09-03`, `/stocks/ali.2023-09-03.xml`)
  - Searches local SQLite database first.
  - If not found, proxies request to `https://phisix-api2.appspot.com/stocks/{symbol}.{date}.json`, returns it, and caches it locally.
- **Stock Archiving (Cron/Manual)**: `GET` or `POST` to `/stocks/archive`
  - Fetches the current stock feed and saves/updates it to the local SQLite database.
- **CORS Permissive**: Allows cross-origin requests.
- **Static Assets**: Serves legacy assets (favicons, apple touch icons, logos, robots.txt, Google verification files) copied from the original project.

## Running Locally

To build and run the application locally outside a container:

```bash
# Run unit tests
cargo test

# Start the server on port 8080
cargo run
```

You can customize the port and database file using environment variables:
```bash
PORT=9090 DATABASE_PATH=my-stocks.db cargo run
```

---

## Containerization with Podman

We provide scripts to easily build, run, stop, and inspect the application inside a Podman container.

### 1. Build the Container Image
Build the multi-stage, secure container image (uses a lightweight Debian runtime image with CA certificates installed for SSL verification):
```bash
./build.sh
```

### 2. Run the Container
Start the container in the background. It maps container port `8080` to host port `8080` and mounts the local `./data` directory to persist the SQLite database (`phisix.db`):
```bash
./run.sh
```

### 3. Check Container Status & Logs
```bash
./status.sh
```

### 4. Stop & Remove the Container
```bash
./stop.sh
```

---

## Technical Details

- **HTTP Framework**: [Axum](https://github.com/tokio-rs/axum) (v0.8) running on [Tokio](https://github.com/tokio-rs/tokio).
- **Web Scraping**: [scraper](https://github.com/caugo/scraper) parsing the CSS selector `#JsonId`.
- **Database**: [SQLite](https://sqlite.org) via [rusqlite](https://github.com/rusqlite/rusqlite) (with bundled sqlite to ensure cross-platform compatibility).
- **Timezone**: Date formats default to the `Asia/Manila` time zone (GMT+8) matching original PSE schedules.
- **XML Serialization**: Implemented with custom JAXB namespace matching to guarantee 100% downstream compatibility.
