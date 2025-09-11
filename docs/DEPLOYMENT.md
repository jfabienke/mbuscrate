# Deployment Guide

This guide covers deploying M-Bus applications built with mbus-rs in production environments.

## Table of Contents
- [System Requirements](#system-requirements)
- [Installation Methods](#installation-methods)
- [Docker Deployment](#docker-deployment)
- [Systemd Service](#systemd-service)
- [Configuration Management](#configuration-management)
- [Monitoring and Logging](#monitoring-and-logging)
- [High Availability](#high-availability)
- [Performance Tuning](#performance-tuning)
- [Security Hardening](#security-hardening)
- [Backup and Recovery](#backup-and-recovery)

## System Requirements

### Minimum Requirements

| Component   | Minimum            | Recommended                |
|-------------|--------------------|----------------------------|
| **CPU**     | 1 core, 1 GHz      | 2+ cores, 2 GHz            |
| **RAM**     | 512 MB             | 2 GB                       |
| **Storage** | 100 MB (app)       | 10 GB (with logs)          |
| **OS**      | Linux kernel 3.10+ | Ubuntu 20.04+ / Debian 11+ |
| **Rust**    | 1.70+              | Latest stable              |

### Operating Systems

**Tested and Supported:**
- Ubuntu 20.04 LTS, 22.04 LTS
- Debian 10, 11, 12
- RHEL/CentOS 8, 9
- Alpine Linux 3.16+ (for containers)
- Raspberry Pi OS (ARM)

- macOS 12+ (development only)

## Installation Methods

### Binary Installation

```bash
# Download pre-built binary (when available)
wget https://github.com/your-org/mbus-rs/releases/latest/download/mbus-rs-linux-amd64
chmod +x mbus-rs-linux-amd64
sudo mv mbus-rs-linux-amd64 /usr/local/bin/mbus-rs

# Or build from source
git clone https://github.com/your-org/mbus-rs.git
cd mbus-rs
cargo build --release
sudo cp target/release/mbus-rs /usr/local/bin/
```

### Package Installation

**Debian/Ubuntu (.deb):**
```bash
# Create debian package
cargo install cargo-deb
cargo deb

# Install
sudo dpkg -i target/debian/mbus-rs_*.deb
```

**RPM-based systems:**
```bash
# Create RPM package
cargo install cargo-rpm
cargo rpm build

# Install
sudo rpm -i target/release/rpms/mbus-rs-*.rpm
```

## Docker Deployment

### Dockerfile

```dockerfile
# Multi-stage build for minimal image
FROM rust:1.75 AS builder

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build release binary
RUN cargo build --release

# Runtime image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 mbus

# Copy binary
COPY --from=builder /app/target/release/mbus-rs /usr/local/bin/

# Set permissions
RUN chmod +x /usr/local/bin/mbus-rs

USER mbus

ENTRYPOINT ["mbus-rs"]
```

### Docker Compose

```yaml
version: '3.8'

services:
  mbus-collector:
    image: mbus-rs:latest
    restart: unless-stopped
    devices:
      - /dev/ttyUSB0:/dev/ttyUSB0
    environment:
      - RUST_LOG=info
      - MBUS_PORT=/dev/ttyUSB0
      - MBUS_BAUD=2400
      - DB_URL=postgresql://mbus:password@db:5432/mbus
    volumes:
      - ./config:/etc/mbus
      - ./logs:/var/log/mbus
    depends_on:
      - db
    networks:
      - mbus-network

  db:
    image: postgres:15-alpine
    restart: unless-stopped
    environment:
      - POSTGRES_DB=mbus
      - POSTGRES_USER=mbus
      - POSTGRES_PASSWORD=password
    volumes:
      - postgres-data:/var/lib/postgresql/data
    networks:
      - mbus-network

  grafana:
    image: grafana/grafana:latest
    restart: unless-stopped
    ports:
      - "3000:3000"
    volumes:
      - grafana-data:/var/lib/grafana
      - ./grafana/dashboards:/etc/grafana/provisioning/dashboards
    networks:
      - mbus-network

volumes:
  postgres-data:
  grafana-data:

networks:
  mbus-network:
    driver: bridge
```

### Building and Running

```bash
# Build image
docker build -t mbus-rs:latest .

# Run with device access
docker run -d \
  --name mbus-collector \
  --device /dev/ttyUSB0 \
  -e RUST_LOG=info \
  -v $(pwd)/config:/etc/mbus \
  mbus-rs:latest

# Using docker-compose
docker-compose up -d

# View logs
docker logs -f mbus-collector
```

## Systemd Service

### Service File

Create `/etc/systemd/system/mbus-collector.service`:

```ini
[Unit]
Description=M-Bus Data Collector
Documentation=https://github.com/your-org/mbus-rs
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=mbus
Group=mbus
WorkingDirectory=/var/lib/mbus

# Environment
Environment="RUST_LOG=info"
Environment="MBUS_CONFIG=/etc/mbus/config.toml"
EnvironmentFile=-/etc/mbus/environment

# Execution
ExecStart=/usr/local/bin/mbus-rs --config /etc/mbus/config.toml
ExecReload=/bin/kill -HUP $MAINPID
Restart=on-failure
RestartSec=10

# Security
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=/var/lib/mbus /var/log/mbus

# Resource limits
LimitNOFILE=65536
MemoryLimit=512M
CPUQuota=50%

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=mbus-collector

[Install]
WantedBy=multi-user.target
```

### Installation and Management

```bash
# Create user and directories
sudo useradd -r -s /bin/false mbus
sudo mkdir -p /var/lib/mbus /var/log/mbus /etc/mbus
sudo chown -R mbus:mbus /var/lib/mbus /var/log/mbus

# Add user to dialout for serial access
sudo usermod -a -G dialout mbus

# Install and enable service
sudo systemctl daemon-reload
sudo systemctl enable mbus-collector
sudo systemctl start mbus-collector

# Check status
sudo systemctl status mbus-collector
sudo journalctl -u mbus-collector -f
```

## Configuration Management

### Configuration File (TOML)

Create `/etc/mbus/config.toml`:

```toml
# General settings
[general]
log_level = "info"
worker_threads = 4

# Serial port configuration
[serial]
port = "/dev/ttyUSB0"
baudrate = 2400
timeout_ms = 1000
retry_count = 3

# Device configuration
[[devices]]
address = 1
name = "Heat Meter 1"
interval_seconds = 300
enabled = true

[[devices]]
address = 2
name = "Water Meter 1"
interval_seconds = 600
enabled = true

# Database configuration
[database]
url = "postgresql://mbus:password@localhost/mbus"
pool_size = 10
connection_timeout = 30

# Monitoring
[monitoring]
enable_metrics = true
metrics_port = 9090
health_check_port = 8080

# Data export
[export]
enable_mqtt = true
mqtt_broker = "tcp://localhost:1883"
mqtt_topic = "mbus/data"
mqtt_qos = 1

enable_influxdb = true
influxdb_url = "http://localhost:8086"
influxdb_token = "your-token"
influxdb_org = "your-org"
influxdb_bucket = "mbus"

# Alerting
[alerting]
enable_email = true
smtp_server = "smtp.gmail.com:587"
smtp_user = "alerts@example.com"
smtp_password = "your-password"
alert_recipients = ["admin@example.com"]

# Thresholds for alerts
[alerting.thresholds]
communication_error_rate = 0.1  # 10%
device_offline_minutes = 30
```

### Environment Variables

Create `/etc/mbus/environment`:

```bash
# Override config file settings
MBUS_PORT=/dev/ttyUSB0
MBUS_BAUD=2400
RUST_LOG=info,mbus_rs=debug
DATABASE_URL=postgresql://mbus:password@localhost/mbus

# Security
MBUS_API_KEY=your-secure-api-key
MBUS_ENCRYPTION_KEY=your-32-byte-key

# Feature flags
ENABLE_DEBUG_MODE=false
ENABLE_METRICS=true
```

### Loading Configuration

```rust
use serde::Deserialize;
use config::{Config, ConfigError, File, Environment};

#[derive(Debug, Deserialize)]
struct AppConfig {
    general: GeneralConfig,
    serial: SerialConfig,
    devices: Vec<DeviceConfig>,
    database: DatabaseConfig,
}

fn load_config() -> Result<AppConfig, ConfigError> {
    let config = Config::builder()
        .add_source(File::with_name("/etc/mbus/config.toml"))
        .add_source(Environment::with_prefix("MBUS"))
        .build()?;

    config.try_deserialize()
}
```

## Monitoring and Logging

### Prometheus Metrics

```rust
use prometheus::{Encoder, TextEncoder, Counter, Gauge, Histogram};

lazy_static! {
    static ref DEVICES_TOTAL: Gauge = register_gauge!(
        "mbus_devices_total",
        "Total number of M-Bus devices"
    ).unwrap();

    static ref READS_TOTAL: Counter = register_counter!(
        "mbus_reads_total",
        "Total number of successful reads"
    ).unwrap();

    static ref READ_DURATION: Histogram = register_histogram!(
        "mbus_read_duration_seconds",
        "Time taken to read from device"
    ).unwrap();
}

// Expose metrics endpoint
async fn metrics_handler() -> String {
    let encoder = TextEncoder::new();
    let metric_families = prometheus::gather();
    let mut buffer = Vec::new();
    encoder.encode(&metric_families, &mut buffer).unwrap();
    String::from_utf8(buffer).unwrap()
}
```

### Structured Logging

```rust
use tracing::{info, warn, error, instrument};
use tracing_subscriber::fmt::json;

fn setup_logging() {
    tracing_subscriber::fmt()
        .json()  // JSON output for log aggregation
        .with_env_filter("info,mbus_rs=debug")
        .init();
}

#[instrument(skip(handle))]
async fn read_device(handle: &mut MBusDeviceHandle, address: u8) {
    info!(device_address = address, "Reading device");

    match send_request(handle, address).await {
        Ok(records) => {
            info!(
                device_address = address,
                record_count = records.len(),
                "Successfully read device"
            );
        }
        Err(e) => {
            error!(
                device_address = address,
                error = %e,
                "Failed to read device"
            );
        }
    }
}
```

### Health Checks

```rust
use axum::{Router, routing::get, Json};
use serde::Serialize;

#[derive(Serialize)]
struct HealthStatus {
    status: String,
    serial_port: bool,
    database: bool,
    last_read: Option<String>,
}

async fn health_check() -> Json<HealthStatus> {
    Json(HealthStatus {
        status: "healthy".to_string(),
        serial_port: check_serial().await,
        database: check_database().await,
        last_read: get_last_read_time().await,
    })
}

// Setup health endpoint
let app = Router::new()
    .route("/health", get(health_check))
    .route("/ready", get(readiness_check));
```

## High Availability

### Multiple Collectors

```yaml
# HAProxy configuration for load balancing
global
    maxconn 4096

defaults
    mode tcp
    timeout connect 5000ms
    timeout client 50000ms
    timeout server 50000ms

frontend mbus_frontend
    bind *:8080
    default_backend mbus_collectors

backend mbus_collectors
    balance roundrobin
    server collector1 192.168.1.10:8080 check
    server collector2 192.168.1.11:8080 check
    server collector3 192.168.1.12:8080 check
```

### Database Replication

```sql
-- PostgreSQL streaming replication setup
-- On primary:
CREATE ROLE replicator WITH REPLICATION LOGIN PASSWORD 'rep_password';

-- postgresql.conf
wal_level = replica
max_wal_senders = 3
wal_keep_segments = 64

-- pg_hba.conf
host replication replicator 192.168.1.0/24 md5
```

### Redis for Caching

```rust
use redis::aio::ConnectionManager;

async fn cache_device_data(
    redis: &ConnectionManager,
    address: u8,
    data: &[MBusRecord]
) -> Result<(), Box<dyn Error>> {
    let key = format!("mbus:device:{}", address);
    let json = serde_json::to_string(data)?;

    redis.set_ex(key, json, 300).await?;  // 5 minute TTL
    Ok(())
}
```

## Performance Tuning

### System Tuning

```bash
# /etc/sysctl.d/99-mbus.conf
# Network tuning
net.core.rmem_max = 134217728
net.core.wmem_max = 134217728
net.ipv4.tcp_rmem = 4096 87380 134217728
net.ipv4.tcp_wmem = 4096 65536 134217728

# File descriptors
fs.file-max = 2097152
fs.nr_open = 2097152

# Apply settings
sudo sysctl -p /etc/sysctl.d/99-mbus.conf
```

### Application Tuning

```rust
// Connection pooling
use deadpool_postgres::{Config, Pool};

fn create_pool() -> Pool {
    let mut cfg = Config::new();
    cfg.dbname = Some("mbus".to_string());
    cfg.host = Some("localhost".to_string());
    cfg.user = Some("mbus".to_string());
    cfg.password = Some("password".to_string());
    cfg.pool = Some(deadpool_postgres::PoolConfig {
        max_size: 32,
        timeouts: Timeouts {
            wait: Some(Duration::from_secs(5)),
            create: Some(Duration::from_secs(5)),
            recycle: Some(Duration::from_secs(5)),
        },
        ..Default::default()
    });

    cfg.create_pool(tokio_postgres::NoTls).unwrap()
}

// Batch processing
async fn batch_read_devices(
    handles: Vec<MBusDeviceHandle>,
    addresses: Vec<u8>
) -> Vec<Result<Vec<MBusRecord>, MBusError>> {
    use futures::stream::{self, StreamExt};

    stream::iter(addresses)
        .map(|addr| async move {
            read_device(&handle, addr).await
        })
        .buffer_unordered(10)  // Process 10 devices concurrently
        .collect()
        .await
}
```

## Security Hardening

### Firewall Rules

```bash
# UFW configuration
sudo ufw default deny incoming
sudo ufw default allow outgoing

# Allow SSH (restrict source IP in production)
sudo ufw allow from 192.168.1.0/24 to any port 22

# Allow monitoring ports (internal only)
sudo ufw allow from 192.168.1.0/24 to any port 9090  # Prometheus
sudo ufw allow from 192.168.1.0/24 to any port 8080  # Health checks

# Enable firewall
sudo ufw enable
```

### AppArmor Profile

Create `/etc/apparmor.d/usr.local.bin.mbus-rs`:

```
#include <tunables/global>

/usr/local/bin/mbus-rs {
  #include <abstractions/base>
  #include <abstractions/nameservice>

  # Binary
  /usr/local/bin/mbus-rs mr,

  # Configuration
  /etc/mbus/** r,

  # Data directories
  /var/lib/mbus/** rw,
  /var/log/mbus/** rw,

  # Serial ports
  /dev/ttyUSB* rw,
  /dev/ttyS* rw,

  # Temp files
  /tmp/** rw,

  # Network
  network inet stream,
  network inet6 stream,
}
```

### Secrets Management

```rust
// Using environment variables
use std::env;

fn get_secret(key: &str) -> Result<String, String> {
    env::var(key).map_err(|_| format!("Missing secret: {}", key))
}

// Using HashiCorp Vault
use vaultrs::client::{VaultClient, VaultClientSettingsBuilder};

async fn get_vault_secret(path: &str) -> Result<String, Box<dyn Error>> {
    let client = VaultClient::new(
        VaultClientSettingsBuilder::default()
            .address("https://vault.example.com:8200")
            .token("your-vault-token")
            .build()?
    )?;

    let secret: Value = client.kv2.read_secret("secret/data/mbus", path).await?;
    Ok(secret["password"].as_str().unwrap().to_string())
}
```

## Backup and Recovery

### Database Backup

```bash
#!/bin/bash
# /usr/local/bin/backup-mbus.sh

BACKUP_DIR="/var/backups/mbus"
DATE=$(date +%Y%m%d_%H%M%S)
DB_NAME="mbus"

# Create backup
pg_dump -U mbus -d $DB_NAME | gzip > "$BACKUP_DIR/mbus_$DATE.sql.gz"

# Keep only last 30 days
find $BACKUP_DIR -name "mbus_*.sql.gz" -mtime +30 -delete

# Sync to S3 (optional)
aws s3 sync $BACKUP_DIR s3://your-bucket/mbus-backups/
```

### Configuration Backup

```bash
# Backup configuration
tar -czf /var/backups/mbus-config-$(date +%Y%m%d).tar.gz \
    /etc/mbus \
    /etc/systemd/system/mbus-collector.service

# Restore configuration
tar -xzf /var/backups/mbus-config-20240101.tar.gz -C /
systemctl daemon-reload
systemctl restart mbus-collector
```

### Disaster Recovery Plan

1. **RPO (Recovery Point Objective)**: 1 hour
2. **RTO (Recovery Time Objective)**: 2 hours

**Recovery Steps:**
```bash
# 1. Provision new server
# 2. Install mbus-rs
wget https://backup.example.com/mbus-rs-latest
chmod +x mbus-rs-latest
sudo mv mbus-rs-latest /usr/local/bin/mbus-rs

# 3. Restore configuration
aws s3 cp s3://your-bucket/mbus-config-latest.tar.gz .
tar -xzf mbus-config-latest.tar.gz -C /

# 4. Restore database
aws s3 cp s3://your-bucket/mbus-backup-latest.sql.gz .
gunzip -c mbus-backup-latest.sql.gz | psql -U mbus -d mbus

# 5. Start services
systemctl start mbus-collector

# 6. Verify operation
curl http://localhost:8080/health
```

## Production Checklist

### Pre-Deployment

- [ ] Hardware tested and verified
- [ ] Serial port permissions configured
- [ ] Database provisioned and backed up
- [ ] Monitoring infrastructure ready
- [ ] Security scan completed
- [ ] Load testing performed
- [ ] Documentation updated
- [ ] Rollback plan prepared

### Deployment

- [ ] Configuration validated
- [ ] Secrets properly managed
- [ ] Service files installed
- [ ] Firewall rules applied
- [ ] SSL certificates installed (if applicable)
- [ ] Monitoring alerts configured
- [ ] Health checks passing

### Post-Deployment

- [ ] Verify all devices communicating
- [ ] Check metrics and logs
- [ ] Validate data in database
- [ ] Test alerting system
- [ ] Document any issues
- [ ] Update runbook

## Related Documentation

- [Hardware Guide](HARDWARE.md) - Hardware setup and compatibility
- [Troubleshooting](TROUBLESHOOTING.md) - Common issues and solutions
- [Security Policy](../SECURITY.md) - Security considerations
- [Configuration Guide](CONFIGURATION.md) - Detailed configuration options
