use axum::{extract::{State, Query}, http::StatusCode, response::IntoResponse, routing::get, Router};
use axum::http::{Request, HeaderMap, HeaderValue};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation, TokenData};
use serde::Deserialize;
use std::sync::Arc;
use std::collections::{HashMap, HashSet};
use tokio::sync::RwLock;
use reqwest::Client;
use std::env;
use std::time::Duration;
use tracing::{info, debug, warn, error};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

#[derive(Debug, Deserialize)]
struct Claims {
    exp: usize,
    iss: String,
    #[serde(deserialize_with = "deserialize_audience")]
    aud: String,
    email: Option<String>
}

// Custom deserializer to handle audience as either string or array
fn deserialize_audience<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct AudienceVisitor;

    impl<'de> Visitor<'de> for AudienceVisitor {
        type Value = String;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a string or array of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(value.to_string())
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: de::SeqAccess<'de>,
        {
            // Take the first audience from the array
            if let Some(first) = seq.next_element::<String>()? {
                Ok(first)
            } else {
                Err(de::Error::custom("audience array is empty"))
            }
        }
    }

    deserializer.deserialize_any(AudienceVisitor)
}

#[derive(Debug, Deserialize)]
struct Jwk {
    kid: String,
    n: String,
    e: String,
    kty: String,
    alg: String,
    #[serde(rename = "use")]
    use_: String,
}

#[derive(Debug, Deserialize)]
struct Jwks {
    keys: Vec<Jwk>,
}

#[derive(Debug, Clone)]
struct AppConfig {
    cf_issuer: String,
    cf_jwks_url: String,
}

impl AppConfig {
    fn from_env() -> Result<Self, String> {
        let team_name = env::var("CF_TEAM_NAME")
            .map_err(|_| "CF_TEAM_NAME environment variable is required".to_string())?;
        
        let cf_issuer = format!("https://{}.cloudflareaccess.com", team_name);
        let cf_jwks_url = format!("https://{}.cloudflareaccess.com/cdn-cgi/access/certs", team_name);
        
        Ok(AppConfig {
            cf_issuer,
            cf_jwks_url,
        })
    }
}

struct AppState {
    jwks: RwLock<HashMap<String, DecodingKey>>,
    client: Client,
    config: Arc<AppConfig>,
}

impl AppState {
    fn new(config: Arc<AppConfig>) -> Self {
        // Configure reqwest client with connection pooling
        let client = Client::builder()
            .pool_max_idle_per_host(10)     // Keep up to 10 idle connections per host
            .pool_idle_timeout(Duration::from_secs(90))  // Keep connections alive for 90 seconds
            .timeout(Duration::from_secs(30))            // Request timeout
            .connect_timeout(Duration::from_secs(10))    // Connection timeout
            .tcp_keepalive(Duration::from_secs(60))      // TCP keep-alive
            .build()
            .expect("Failed to create HTTP client");

        Self {
            jwks: RwLock::new(HashMap::new()),
            client,
            config,
        }
    }

    // Fetch and cache all JWKS keys
    async fn fetch_and_cache_keys(&self) -> Result<(), ()> {
        info!("Fetching JWKS keys from: {}", self.config.cf_jwks_url);
        
        let keys = fetch_jwks(&self.client, &self.config.cf_jwks_url).await?;
        info!("Successfully fetched {} keys from JWKS", keys.len());
        
        let mut jwks = self.jwks.write().await;
        jwks.clear(); // Clear existing cache
        
        let mut successful_keys = 0;
        for jwk in &keys {
            // Validate JWK properties before processing
            if jwk.kty != "RSA" {
                warn!("Skipping non-RSA key: {} (type: {})", jwk.kid, jwk.kty);
                continue;
            }
            
            if jwk.alg != "RS256" {
                warn!("Skipping non-RS256 key: {} (algorithm: {})", jwk.kid, jwk.alg);
                continue;
            }
            
            if jwk.use_ != "sig" {
                warn!("Skipping non-signature key: {} (use: {})", jwk.kid, jwk.use_);
                continue;
            }
            
            if let Ok(decoding_key) = DecodingKey::from_rsa_components(&jwk.n, &jwk.e) {
                jwks.insert(jwk.kid.clone(), decoding_key);
                debug!("Cached key: {}", jwk.kid);
                successful_keys += 1;
            } else {
                warn!("Failed to process RSA components for key: {}", jwk.kid);
            }
        }
        
        info!("Successfully cached {}/{} JWKS keys", successful_keys, keys.len());
        Ok(())
    }

    // Start the periodic key refresh task
    fn start_key_refresh_task(self: Arc<Self>) {
        let state = Arc::clone(&self);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(12 * 60 * 60)); // 12 hours
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            
            loop {
                interval.tick().await;
                info!("Starting periodic JWKS key refresh");
                
                match state.fetch_and_cache_keys().await {
                    Ok(()) => info!("Periodic JWKS key refresh completed successfully"),
                    Err(()) => error!("Periodic JWKS key refresh failed"),
                }
            }
        });
    }
}

#[tokio::main]
async fn main() {
    // Initialize tracing with environment variable support
    // Set RUST_LOG=debug or RUST_LOG=info to control log level
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env().add_directive("nginx_cloudflare_access_jwt_validator=info".parse().unwrap()))
        .init();

    let config = Arc::new(AppConfig::from_env().expect("Failed to load configuration from environment"));
    info!("Using Cloudflare team: {}", config.cf_issuer);
    
    let state = Arc::new(AppState::new(config));
    
    // Fetch and cache keys at startup
    info!("Initializing JWKS key cache at startup");
    if let Err(()) = state.fetch_and_cache_keys().await {
        error!("Failed to fetch JWKS keys at startup - continuing anyway");
    }
    
    // Start periodic key refresh task
    info!("Starting periodic JWKS key refresh task (every 12 hours)");
    Arc::clone(&state).start_key_refresh_task();
    
    let app = Router::new()
        .route("/auth", get(auth_handler))
        .route("/health", get(health_handler))
        .route("/refresh-keys", get(refresh_keys_handler))
        .with_state(state);
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();
    info!("Starting server on 0.0.0.0:8080 with connection pooling enabled");
    
    // Configure the server - Axum handles HTTP/1.1 keep-alive by default
    axum::serve(listener, app).await.unwrap();
}

#[derive(Debug, Deserialize)]
struct AuthQuery {
    aud: Option<String>,
}

async fn auth_handler(
    State(state): State<Arc<AppState>>,
    Query(query): Query<AuthQuery>,
    req: Request<axum::body::Body>
) -> impl IntoResponse {
    // Extract audience from header or query parameter
    let aud = req.headers().get("x-expected-audience").and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or(query.aud.clone());
    let Some(aud) = aud else {
        debug!("Missing X-Expected-Audience header and 'aud' query parameter");
        debug!("Available headers:");
        for (name, value) in req.headers() {
            if let Ok(val_str) = value.to_str() {
                debug!("  {}: {}", name, val_str);
            }
        }
        return create_response(StatusCode::UNAUTHORIZED);
    };
    debug!("Expected audience: {}", aud);
    
    // Extract JWT from CF_Authorization header or cookie
    let jwt = extract_jwt(&req);
    if jwt.is_none() {
        debug!("No JWT found in headers or cookies");
        return create_response(StatusCode::UNAUTHORIZED);
    }
    let jwt = jwt.unwrap();
    debug!("JWT found: {}...", &jwt[..jwt.len().min(50)]);
    
    // Validate JWT
    match validate_jwt(&jwt, &state, &aud).await {
        Ok(_) => {
            debug!("JWT validation successful");
            create_response(StatusCode::NO_CONTENT) // 204 for Nginx auth_request
        },
        Err(_) => {
            warn!("JWT validation failed");
            create_response(StatusCode::UNAUTHORIZED)
        },
    }
}

// Health check endpoint for load balancers and monitoring
async fn health_handler() -> impl IntoResponse {
    create_response(StatusCode::OK)
}

// Manual key refresh endpoint for operational purposes
async fn refresh_keys_handler(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    info!("Manual JWKS key refresh requested");
    
    match state.fetch_and_cache_keys().await {
        Ok(()) => {
            info!("Manual JWKS key refresh completed successfully");
            create_response(StatusCode::OK)
        },
        Err(()) => {
            error!("Manual JWKS key refresh failed");
            create_response(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// Helper function to create responses with appropriate headers for connection reuse
fn create_response(status: StatusCode) -> (StatusCode, HeaderMap) {
    let mut headers = HeaderMap::new();
    
    // Set Connection: keep-alive to encourage connection reuse
    headers.insert("connection", HeaderValue::from_static("keep-alive"));
    
    // Set Keep-Alive header with timeout and max requests
    // Optimized for nginx: longer timeout, more requests per connection
    headers.insert("keep-alive", HeaderValue::from_static("timeout=120, max=10000"));
    
    // Disable caching for auth responses
    headers.insert("cache-control", HeaderValue::from_static("no-cache, no-store, must-revalidate"));
    headers.insert("pragma", HeaderValue::from_static("no-cache"));
    headers.insert("expires", HeaderValue::from_static("0"));
    
    (status, headers)
}

fn extract_jwt(req: &Request<axum::body::Body>) -> Option<String> {
    // Try header first
    if let Some(auth) = req.headers().get("cf-authorization") {
        if let Ok(s) = auth.to_str() {
            return Some(s.to_string());
        }
    }
    // Try cookie
    if let Some(cookie) = req.headers().get("cookie") {
        if let Ok(cookie_str) = cookie.to_str() {
            for part in cookie_str.split(';') {
                let part = part.trim();
                if part.starts_with("CF_Authorization=") {
                    return Some(part["CF_Authorization=".len()..].to_string());
                }
            }
        }
    }
    None
}

async fn validate_jwt(token: &str, state: &AppState, aud: &str) -> Result<TokenData<Claims>, ()> {
    // Decode header to get kid
    let header = decode_header(token).map_err(|e| {
        error!("Failed to decode JWT header: {:?}", e);
        ()
    })?;
    let kid = header.kid.ok_or_else(|| {
        error!("No 'kid' field in JWT header");
        ()
    })?;
    debug!("JWT kid: {}", kid);
    
    // Get decoding key from cache
    let key = {
        let jwks = state.jwks.read().await;
        jwks.get(&kid).cloned()
    };
    
    let decoding_key = if let Some(key) = key {
        debug!("Using cached key for kid: {}", kid);
        key
    } else {
        warn!("Key '{}' not found in cache, attempting to refresh JWKS", kid);
        // Try to refresh the cache in case new keys were added
        if let Err(()) = state.fetch_and_cache_keys().await {
            error!("Failed to refresh JWKS cache");
            return Err(());
        }
        
        // Try to get the key again after refresh
        let jwks = state.jwks.read().await;
        jwks.get(&kid).cloned().ok_or_else(|| {
            error!("Key '{}' still not found after JWKS refresh", kid);
            ()
        })?
    };
    
    // Validate
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[aud]);
    validation.iss = Some(HashSet::from([state.config.cf_issuer.clone()]));
    
    debug!("Validating JWT with audience: {} and issuer: {}", aud, state.config.cf_issuer);
    let token_data = decode::<Claims>(token, &decoding_key, &validation).map_err(|e| {
        error!("JWT validation error: {:?}", e);
        ()
    })?;
    
    // Additional validation of claims
    let claims = &token_data.claims;
    
    // Validate expiration (jsonwebtoken already checks this, but we can log it)
    debug!("JWT expires at: {}", claims.exp);
    
    // Validate issuer (also checked by jsonwebtoken, but we can verify it matches our expectation)
    if claims.iss != state.config.cf_issuer {
        error!("JWT issuer mismatch: expected {}, got {}", state.config.cf_issuer, claims.iss);
        return Err(());
    }
    
    // Validate audience (also checked by jsonwebtoken)
    if claims.aud != aud {
        error!("JWT audience mismatch: expected {}, got {}", aud, claims.aud);
        return Err(());
    }
    
    // Log email if present
    if let Some(email) = &claims.email {
        debug!("JWT validated for user: {}", email);
    }
    
    Ok(token_data)
}

// fetch_jwks with detailed error logging
async fn fetch_jwks(client: &Client, url: &str) -> Result<Vec<Jwk>, ()> {
    debug!("Making HTTP request to: {}", url);
    let resp = client.get(url).send().await.map_err(|e| {
        error!("HTTP request failed: {:?}", e);
        ()
    })?;
    
    debug!("HTTP response status: {}", resp.status());
    if !resp.status().is_success() {
        error!("HTTP request returned non-success status: {}", resp.status());
        return Err(());
    }
    
    let jwks: Jwks = resp.json().await.map_err(|e| {
        error!("Failed to parse JSON response: {:?}", e);
        ()
    })?;
    
    debug!("Successfully parsed JWKS with {} keys", jwks.keys.len());
    Ok(jwks.keys)
}
