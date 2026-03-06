# Running Drasi Server with Docker

This guide covers running Drasi Server using Docker and Docker Compose.

## Prerequisites

- [Docker](https://docs.docker.com/get-docker/) (20.10+)
- [Docker Compose](https://docs.docker.com/compose/install/) (v2.0+)

## Quick Start

```bash
# Clone the repository with submodules
git clone --recurse-submodules https://github.com/drasi-project/drasi-server.git
cd drasi-server

# Copy environment template
cp .env.example .env

# (Optional) Edit .env with your settings
# nano .env

# Start the full stack (Drasi Server + PostgreSQL)
docker compose up -d

# Check status
docker compose ps

# View logs
docker compose logs -f drasi-server

# Open API documentation
open http://localhost:8080/api/v1/docs/
```

## Configuration

### Editing Configuration

Configuration is managed via **volume mounting**. The `config/` directory on your host is mounted into the container at `/app/config/`.

**To modify configuration:**

1. Edit `config/server.yaml` on your host machine using your favorite editor
2. Restart the server: `docker compose restart drasi-server`

```bash
# Edit configuration
nano config/server.yaml

# Apply changes
docker compose restart drasi-server
```

### Environment Variables

Environment variables can be set in two ways:

1. **`.env` file** (recommended) - Copy `.env.example` to `.env` and customize
2. **Command line** - Pass via `docker compose` or `docker run`

Key environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `DRASI_API_PORT` | Host port for REST API | `8080` |
| `DRASI_SSE_PORT` | Host port for SSE reactions | `8081` |
| `LOG_LEVEL` | Log level (trace/debug/info/warn/error) | `info` |
| `POSTGRES_HOST` | PostgreSQL host | `postgres` |
| `POSTGRES_PORT` | PostgreSQL port | `5432` |
| `POSTGRES_DATABASE` | Database name | `drasi` |
| `POSTGRES_USER` | Database user | `drasi_user` |
| `POSTGRES_PASSWORD` | Database password | `drasi_password` |

### Using Environment Variables in Config

Your `config/server.yaml` can reference environment variables:

```yaml
sources:
  - kind: postgres
    id: my-db
    host: "${DB_HOST}"
    port: "${DB_PORT:-5432}"
    database: "${DB_NAME}"
    user: "${DB_USER}"
    password: "${DB_PASSWORD}"
```

## Deployment Options

### Full Stack (Server + PostgreSQL)

Includes Drasi Server and PostgreSQL with CDC (Change Data Capture) support:

```bash
docker compose up -d
```

### Server Only (BYO Database)

Use this when you have your own PostgreSQL or other data source:

```bash
docker compose -f docker-compose.server-only.yml up -d
```

Configure your database connection in `.env` or `config/server.yaml`.

## Building

### Build from Source

```bash
# Build the Docker image
docker compose build

# Or build without cache
docker compose build --no-cache
```

### Build and Tag Manually

```bash
docker build -t drasi-server:latest .
docker build -t drasi-server:v0.1.0 .
```

## Common Operations

### View Logs

```bash
# All services
docker compose logs -f

# Just Drasi Server
docker compose logs -f drasi-server

# With timestamps
docker compose logs -f -t drasi-server
```

### Check Health

```bash
# Container status
docker compose ps

# Health endpoint
curl http://localhost:8080/health

# Detailed status
docker inspect drasi-server | jq '.[0].State.Health'
```

### Restart Services

```bash
# Restart Drasi Server (apply config changes)
docker compose restart drasi-server

# Restart all services
docker compose restart
```

### Stop and Clean Up

```bash
# Stop services
docker compose down

# Stop and remove volumes (WARNING: deletes data)
docker compose down -v

# Remove built images
docker compose down --rmi local
```

## Connecting to PostgreSQL

The included PostgreSQL container has CDC enabled:

```bash
# Connect via psql
docker exec -it drasi-postgres psql -U drasi_user -d drasi

# Or from host (requires psql installed)
psql -h localhost -U drasi_user -d drasi
```

## Troubleshooting

### Container Won't Start

```bash
# Check logs for errors
docker compose logs drasi-server

# Common issues:
# - Config file syntax error: check config/server.yaml
# - Port already in use: change DRASI_API_PORT in .env
# - Database not ready: wait for postgres health check
```

### Configuration Not Applied

```bash
# Restart server to apply config changes
docker compose restart drasi-server

# Verify config is mounted
docker exec drasi-server cat /app/config/server.yaml
```

### Database Connection Issues

```bash
# Check PostgreSQL is running
docker compose ps postgres

# Check connection from server container
docker exec drasi-server curl -s postgres:5432 || echo "Cannot reach postgres"

# View PostgreSQL logs
docker compose logs postgres
```

### Permission Denied on Config

If you see permission errors, ensure the config directory is readable:

```bash
chmod -R 755 config/
```

## Production Considerations

### Security

1. **Change default passwords** in `.env`
2. **Don't commit `.env`** - it's gitignored by default
3. Consider using Docker secrets for sensitive values
4. Run behind a reverse proxy (nginx, traefik) for TLS

### Persistence

- PostgreSQL data is stored in a named volume (`drasi_postgres_data`)
- Config files are on the host filesystem
- Logs are accessible via `docker compose logs`

### Scaling

For production deployments, consider:
- External PostgreSQL with high availability
- Container orchestration (Kubernetes, Docker Swarm)
- Load balancing for multiple Drasi Server instances

## API Endpoints

Once running, the following endpoints are available:

| Endpoint | Description |
|----------|-------------|
| `http://localhost:8080/health` | Health check |
| `http://localhost:8080/api/v1/docs/` | API documentation (Swagger UI) |
| `http://localhost:8080/api/v1/openapi.json` | OpenAPI spec |
| `http://localhost:8080/api/v1/sources` | Source management |
| `http://localhost:8080/api/v1/queries` | Query management |
| `http://localhost:8080/api/v1/reactions` | Reaction management |
