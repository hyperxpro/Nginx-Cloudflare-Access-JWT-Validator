# Nginx Cloudflare Access JWT Validator

A high-performance Rust-based JWT validation service designed for seamless integration with Nginx's `auth_request` module and Cloudflare Access authentication. This service provides efficient validation of Cloudflare Access JWTs with optimized connection pooling and caching for production environments.

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust Version](https://img.shields.io/badge/rust-1.76+-blue.svg)](https://www.rust-lang.org)

## Features

- **Fast JWT Validation**: Optimized for high-throughput authentication requests
- **Cloudflare Access Integration**: Native support for Cloudflare Access JWTs
- **JWKS Caching**: Intelligent caching with automatic key rotation and refresh
- **Nginx Integration**: Purpose-built for Nginx `auth_request` module
- **Connection Pooling**: HTTP/1.1 keep-alive optimization for reduced latency
- **Health Monitoring**: Built-in health check and metrics endpoints
- **Production Ready**: Comprehensive logging, error handling, and monitoring
- **Docker Support**: Multi-stage builds with security-optimized distroless images

## Quick Start

### Prerequisites

- Rust 1.70+ (for building from source)
- Docker (for containerized deployment)
- Cloudflare Access configured for your domain

### Environment Variables

| Variable | Required | Description | Example |
|----------|----------|-------------|---------|
| `CF_TEAM_NAME` | Yes | Your Cloudflare team name | `mycompany` |
| `RUST_LOG` | No | Log level (debug, info, warn, error) | `info` |

### Docker Deployment (Recommended)

```bash
# Pull and run the container
docker run -d \
  --name cf-jwt-validator \
  -p 8080:8080 \
  -e CF_TEAM_NAME=your-team-name \
  -e RUST_LOG=info \
  hyperxpro/nginx-cloudflare-access-jwt-validator:latest
```

### Docker Compose

Create a `docker-compose.yml` file (or use the provided example):

```yaml
services:
  cf-jwt-validator:
    image: hyperxpro/nginx-cloudflare-access-jwt-validator:latest
    container_name: cf-jwt-validator
    restart: unless-stopped
    ports:
      - "8080:8080"
    environment:
      - CF_TEAM_NAME=your-team-name
      - RUST_LOG=info
    healthcheck:
      test: ["CMD-SHELL", "curl -f http://localhost:8080/health || exit 1"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
```

Run with Docker Compose:

```bash
# Start the service
docker compose up -d

# View logs
docker compose logs -f cf-jwt-validator

# Stop the service
docker compose down
```

**Note**: Example `docker-compose.yml` and `nginx.conf` files are included in the repository.

### Building from Source

```bash
# Clone the repository
git clone https://github.com/hyperxpro/Nginx-Cloudflare-Access-JWT-Validator.git
cd Nginx-Cloudflare-Access-JWT-Validator

# Build and run
cargo build --release
$env:CF_TEAM_NAME="your-team-name"; .\target\release\Nginx-Cloudflare-Access-JWT-Validator.exe
```

### Docker Build

```bash
# Build the Docker image
docker build -t nginx-cloudflare-access-jwt-validator .

# Run the container
docker run -d \
  --name cf-jwt-validator \
  -p 8080:8080 \
  -e CF_TEAM_NAME=your-team-name \
  nginx-cloudflare-access-jwt-validator
```

## API Endpoints

### Authentication Endpoint
- **GET** `/auth`
- **Purpose**: Validates JWT tokens for Nginx auth_request
- **Headers**:
  - `CF-Authorization`: JWT token (or via cookie)
  - `X-Expected-Audience`: Expected audience claim (optional if using query parameter)
- **Query Parameters**:
  - `aud`: Expected audience claim (optional if using header)
- **How Audience is Determined**: The service will use the `X-Expected-Audience` header if present, otherwise it will use the `aud` query parameter. If neither is provided, the request is rejected with `401 Unauthorized`.
- **Responses**:
  - `204 No Content`: Valid JWT
  - `401 Unauthorized`: Invalid or missing JWT

### Health Check
- **GET** `/health`
- **Purpose**: Health monitoring for load balancers
- **Response**: `200 OK`

### Manual Key Refresh
- **GET** `/refresh-keys`
- **Purpose**: Force refresh of JWKS cache
- **Responses**:
  - `200 OK`: Refresh successful
  - `500 Internal Server Error`: Refresh failed

## Nginx Configuration

### Basic Auth Request Setup

```nginx
server {
    listen 80;
    server_name your-app.example.com;

    # Internal auth endpoint
    location = /auth {
        internal;
        proxy_pass http://cf-jwt-validator:8080/auth;
        proxy_pass_request_body off;
        proxy_set_header Content-Length "";
        proxy_set_header X-Original-URI $request_uri;
        proxy_set_header X-Expected-Audience "your-app-audience-tag";
        
        # Pass through Cloudflare headers
        proxy_set_header CF-Authorization $http_cf_authorization;
        proxy_set_header Cookie $http_cookie;
    }

    # Protected location
    location / {
        auth_request /auth;
        
        # Your application backend
        proxy_pass http://your-backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### Advanced Configuration with Error Handling

```nginx
upstream cf_jwt_validator {
    server cf-jwt-validator:8080 max_fails=3 fail_timeout=30s;
    keepalive 32;
}

server {
    listen 80;
    server_name your-app.example.com;

    # Health check for the validator
    location = /validator-health {
        proxy_pass http://cf_jwt_validator/health;
        access_log off;
    }

    # Internal auth endpoint with error handling
    location = /auth {
        internal;
        proxy_pass http://cf_jwt_validator/auth;
        proxy_pass_request_body off;
        proxy_set_header Content-Length "";
        proxy_set_header X-Original-URI $request_uri;
        proxy_set_header X-Expected-Audience "your-app-audience-tag";
        
        # Connection optimization
        proxy_http_version 1.1;
        proxy_set_header Connection "";
        
        # Pass through authentication headers
        proxy_set_header CF-Authorization $http_cf_authorization;
        proxy_set_header Cookie $http_cookie;
        
        # Timeouts
        proxy_connect_timeout 5s;
        proxy_read_timeout 10s;
        proxy_send_timeout 5s;
    }

    # Custom error page for authentication failures
    error_page 401 = @error401;
    location @error401 {
        return 302 https://your-team-name.cloudflareaccess.com;
    }

    # Protected application
    location / {
        auth_request /auth;
        
        # Set user info from JWT (optional)
        auth_request_set $user $upstream_http_x_user_email;
        proxy_set_header X-User-Email $user;
        
        proxy_pass http://your-backend;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

## Configuration Details

### JWT Token Sources

The validator looks for JWT tokens in the following order:

1. **CF-Authorization Header**: `CF-Authorization: <jwt-token>`
2. **Cookie**: `CF_Authorization=<jwt-token>`

### Audience Validation

The expected audience must be provided via the `X-Expected-Audience` header. This should match your Cloudflare Access application's audience tag.

### JWKS Key Management

- **Automatic Refresh**: Keys are refreshed every 12 hours
- **Cache Miss Handling**: If a required key is not found, the cache is refreshed immediately
- **Key Validation**: Only RSA/RS256 signature keys are cached
- **Fallback**: Service continues operating even if key refresh fails

## Performance Optimizations

### Connection Pooling

- **HTTP Client**: Configurable connection pool with keep-alive
- **Nginx Integration**: Optimized headers for connection reuse
- **Timeout Settings**: Balanced for performance and reliability

### Caching Strategy

- **JWKS Cache**: In-memory cache with RwLock for concurrent access
- **Key Rotation**: Automatic background refresh without blocking requests
- **Memory Efficiency**: Only valid keys are stored in cache

### Resource Utilization

- **Async Runtime**: Tokio-based for high concurrency
- **Memory Management**: Minimal allocations in hot paths
- **CPU Efficiency**: Optimized JWT validation pipeline

## Monitoring and Observability

### Logging

The service uses structured logging with the `tracing` crate:

```powershell
# Set log level via environment variable
$env:RUST_LOG="debug"  # debug, info, warn, error

# Module-specific logging
$env:RUST_LOG="nginx_cloudflare_access_jwt_validator=debug,reqwest=info"
```

### Health Monitoring

```bash
# Check service health
curl http://localhost:8080/health

# Force key refresh (operational endpoint)
curl http://localhost:8080/refresh-keys
```

### Key Metrics to Monitor

- **Request Latency**: `/auth` endpoint response times
- **Success Rate**: Ratio of 204 vs 401 responses
- **Cache Hit Rate**: Frequency of JWKS cache refreshes
- **Connection Pool**: HTTP client connection utilization

## Security Considerations

### Runtime Security

- **Distroless Image**: Minimal attack surface with no shell or package manager
- **Non-root User**: Runs as unprivileged user in container
- **Resource Limits**: Configure appropriate CPU/memory limits

### Network Security

- **Internal Only**: Auth endpoint should not be publicly accessible
- **TLS**: Use HTTPS for all external communications
- **Firewall**: Restrict access to validator service

### JWT Validation

- **Signature Verification**: Full RSA signature validation
- **Expiration Checking**: Automatic token expiration handling
- **Issuer Validation**: Strict issuer verification against Cloudflare
- **Audience Validation**: Configurable audience claim verification

## Troubleshooting

### Common Issues

#### 1. "CF_TEAM_NAME environment variable is required"
```powershell
# Ensure the environment variable is set
$env:CF_TEAM_NAME="your-team-name"
```

#### 2. "Failed to fetch JWKS keys at startup"
- Check internet connectivity to Cloudflare
- Verify team name is correct
- Check firewall/proxy settings

#### 3. "Key 'xxx' not found in cache"
- Check if JWKS endpoint is accessible
- Verify JWT header contains valid 'kid' field
- Monitor logs for key refresh attempts

### Debug Mode

Enable detailed logging for troubleshooting:

```powershell
$env:RUST_LOG="debug"
.\target\release\Nginx-Cloudflare-Access-JWT-Validator.exe
```

### Testing the Service

```bash
# Test health endpoint
curl -v http://localhost:8080/health

# Test auth endpoint (will return 401 without valid JWT)
curl -v -H "X-Expected-Audience: your-audience" http://localhost:8080/auth

# Test with JWT token
curl -v \
  -H "CF-Authorization: eyJ..." \
  -H "X-Expected-Audience: your-audience" \
  http://localhost:8080/auth
```

## Development

### Building

```powershell
# Debug build
cargo build

# Release build
cargo build --release

# Run the application locally
$env:CF_TEAM_NAME="your-team-name"; $env:RUST_LOG="debug"; cargo run
```

### Project Structure

```
src/
├── main.rs          # Main application code
Cargo.toml           # Dependencies and metadata
Dockerfile           # Multi-stage Docker build
docker-compose.yml   # Example Docker Compose setup
nginx.conf           # Example Nginx configuration
LICENSE              # Apache 2.0 license
README.md            # This file
```

## License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

## Support

For issues and questions:

- **GitHub Issues**: [Report bugs or request features](https://github.com/hyperxpro/Nginx-Cloudflare-Access-JWT-Validator/issues)
- **Documentation**: This README and inline code comments
- **Community**: Discussions tab on GitHub repository

## Changelog

### Version 0.2.0
- Added support for passing the expected audience via the `aud` query parameter as an alternative to the `X-Expected-Audience` header in the `/auth` endpoint.
- Updated documentation to reflect this new feature.

### Version 0.1.0
- Initial release
- Core JWT validation functionality
- JWKS caching with automatic refresh
- Nginx auth_request integration
- Docker support with distroless images
- Comprehensive logging and monitoring
