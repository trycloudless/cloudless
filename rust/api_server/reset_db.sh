#!/bin/bash
set -e

DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

# Read DATABASE_URL from .env
DB_URL=$(grep -E '^DATABASE_URL=' "$DIR/.env" | sed 's/^DATABASE_URL=//' | tr -d '"')

if [ -z "$DB_URL" ]; then
    echo "Error: DATABASE_URL not found in $DIR/.env"
    exit 1
fi

# Extract components from the URL
# Format: postgres://user:pass@host:port/dbname?params
DB_NAME=$(echo "$DB_URL" | sed 's|.*://.*/\([^?]*\).*|\1|')
USER_PASS=$(echo "$DB_URL" | sed 's|postgres://\([^@]*\)@.*|\1|')
HOST_PORT=$(echo "$DB_URL" | sed 's|.*@\([^/]*\)/.*|\1|')
HOST_PORT_NUM=$(echo "$HOST_PORT" | grep -o '[0-9]*$')

# Inside the container, Postgres listens on localhost:5432
CONTAINER_URL="postgres://${USER_PASS}@localhost:5432/postgres"
CONTAINER_ID=$(docker ps -qf "publish=${HOST_PORT_NUM}")

if [ -z "$CONTAINER_ID" ]; then
    echo "Error: No Docker container found with port $HOST_PORT_NUM"
    exit 1
fi

echo "Dropping and recreating database: $DB_NAME"

docker exec -i "$CONTAINER_ID" psql "$CONTAINER_URL" <<SQL
-- Terminate all connections to the database
SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '$DB_NAME' AND pid <> pg_backend_pid();
DROP DATABASE IF EXISTS $DB_NAME;
CREATE DATABASE $DB_NAME;
SQL

echo "Database '$DB_NAME' has been reset. Migrations will run on next API server start."
