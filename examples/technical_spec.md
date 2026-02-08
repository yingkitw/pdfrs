# API Gateway Technical Specification v2.0

## Overview

The API Gateway serves as the **single entry point** for all client requests. It handles *routing*, *authentication*, *rate limiting*, and *request transformation*.

## System Requirements

### Hardware Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 4 cores | 8 cores |
| RAM | 8 GB | 16 GB |
| Storage | 50 GB SSD | 200 GB NVMe |
| Network | 1 Gbps | 10 Gbps |

### Software Dependencies

- **Runtime**: Rust 1.75+ with `tokio` async runtime
- **Database**: PostgreSQL 16 with `pgbouncer` connection pooling
- **Cache**: Redis 7.2 with cluster mode enabled
- **Message Queue**: Apache Kafka 3.6

## Architecture

### Request Flow

1. Client sends HTTPS request to load balancer
2. Load balancer forwards to gateway instance
3. Gateway performs authentication via JWT validation
4. Rate limiter checks request quota
5. Request transformer normalizes the payload
   1. Header injection for downstream services
   2. Body schema validation against OpenAPI spec
   3. Query parameter sanitization
6. Router dispatches to appropriate backend service
7. Response transformer formats the output
8. Gateway returns response to client

### Core Components

#### Authentication Module

```rust
pub trait Authenticator: Send + Sync {
    async fn validate_token(&self, token: &str) -> Result<Claims, AuthError>;
    async fn refresh_token(&self, refresh: &str) -> Result<TokenPair, AuthError>;
    async fn revoke_token(&self, token: &str) -> Result<(), AuthError>;
}

pub struct JwtAuthenticator {
    secret: Vec<u8>,
    issuer: String,
    audience: Vec<String>,
    expiry_seconds: u64,
}

impl Authenticator for JwtAuthenticator {
    async fn validate_token(&self, token: &str) -> Result<Claims, AuthError> {
        let decoded = jsonwebtoken::decode::<Claims>(
            token,
            &DecodingKey::from_secret(&self.secret),
            &Validation::new(Algorithm::HS256),
        )?;
        Ok(decoded.claims)
    }
}
```

#### Rate Limiter

```rust
pub struct SlidingWindowRateLimiter {
    window_size: Duration,
    max_requests: u64,
    store: Arc<dyn RateLimitStore>,
}

impl SlidingWindowRateLimiter {
    pub async fn check_limit(&self, key: &str) -> Result<RateLimitResult, Error> {
        let now = SystemTime::now();
        let window_start = now - self.window_size;
        let count = self.store.count_requests(key, window_start, now).await?;
        
        if count >= self.max_requests {
            Ok(RateLimitResult::Exceeded {
                retry_after: self.calculate_retry_after(key).await?,
            })
        } else {
            self.store.record_request(key, now).await?;
            Ok(RateLimitResult::Allowed {
                remaining: self.max_requests - count - 1,
            })
        }
    }
}
```

### Configuration Schema

```json
{
  "gateway": {
    "listen_address": "0.0.0.0:8443",
    "tls": {
      "cert_path": "/etc/certs/server.crt",
      "key_path": "/etc/certs/server.key"
    },
    "rate_limit": {
      "window_seconds": 60,
      "max_requests": 1000
    },
    "routes": [
      {
        "path": "/api/v2/users",
        "upstream": "http://user-service:8080",
        "methods": ["GET", "POST", "PUT", "DELETE"],
        "auth_required": true
      }
    ]
  }
}
```

## Error Codes

| Code | Name | Description |
|------|------|-------------|
| 1001 | AUTH_INVALID_TOKEN | The provided JWT token is invalid or expired |
| 1002 | AUTH_MISSING_TOKEN | No authentication token was provided |
| 1003 | RATE_LIMIT_EXCEEDED | Request rate limit has been exceeded |
| 1004 | ROUTE_NOT_FOUND | No matching route for the request path |
| 1005 | UPSTREAM_TIMEOUT | Backend service did not respond in time |
| 1006 | UPSTREAM_ERROR | Backend service returned an error |
| 1007 | VALIDATION_FAILED | Request payload failed schema validation |
| 1008 | CIRCUIT_OPEN | Circuit breaker is open for this service |

## Monitoring and Observability

### Key Metrics

- **Request rate**: Requests per second by route and method
- **Latency**: P50, P90, P95, P99 latency distributions
- **Error rate**: 4xx and 5xx error percentages
- **Circuit breaker state**: Open/closed/half-open per upstream
- **Connection pool**: Active, idle, and waiting connections

### Health Check Endpoint

```
GET /health
```

Response:

```json
{
  "status": "healthy",
  "version": "2.0.0",
  "uptime_seconds": 86400,
  "checks": {
    "database": "ok",
    "cache": "ok",
    "upstream_services": {
      "user-service": "ok",
      "data-service": "ok",
      "auth-service": "degraded"
    }
  }
}
```

## Security Considerations

- All traffic **must** use TLS 1.3
- JWT tokens expire after *15 minutes*
- Refresh tokens are single-use with `rotation policy`
- IP-based rate limiting prevents DDoS attacks
- Request body size limited to **10 MB**
- SQL injection protection via parameterized queries
- XSS prevention through output encoding

---

*Document version 2.0 - Last updated 2025-12-15*
