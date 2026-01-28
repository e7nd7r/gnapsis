# Gnapsis development tasks

# Default recipe - show available commands
default:
    @just --list

# --- Database ---

# Start the AGE database
db-up:
    docker compose up -d age
    @echo "Waiting for AGE to be ready..."
    @until docker compose exec -T age pg_isready -U postgres > /dev/null 2>&1; do sleep 1; done
    @echo "AGE is ready at localhost:5432"

# Stop the AGE database
db-down:
    docker compose down

# Stop and remove all data
db-reset:
    docker compose down -v
    @echo "Database data removed"

# Show database logs
db-logs:
    docker compose logs -f age

# Connect to psql (interactive)
db-shell:
    docker compose exec age psql -U postgres -d gnapsis_dev

# Run a SQL command
db-sql cmd:
    @docker compose exec -T age psql -U postgres -d gnapsis_dev -c "{{cmd}}"

# Run a Cypher query (usage: just cypher "MATCH (n) RETURN n")
cypher query:
    @docker compose exec -T age psql -U postgres -d gnapsis_dev -c "LOAD 'age'; SET search_path = ag_catalog, public; SELECT * FROM cypher('knowledge_graph', \$\$ {{query}} \$\$) as (result agtype);"

# --- Build & Test ---

# Build the project
build:
    cargo build

# Run all tests
test:
    cargo test

# Run tests with output
test-verbose:
    cargo test -- --nocapture

# Run integration tests (requires db-up)
test-integration:
    cargo test --features integration -- --ignored

# Check code without building
check:
    cargo check

# Format code
fmt:
    cargo fmt

# Run clippy
lint:
    cargo clippy -- -D warnings

# --- Development ---

# Run the MCP server
run:
    cargo run

# Watch and rebuild on changes
watch:
    cargo watch -x check -x test

# Clean build artifacts
clean:
    cargo clean
