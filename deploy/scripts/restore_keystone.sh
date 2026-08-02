#!/bin/bash
# Restore script for Keystone database (tpt-data volume)
# Usage: ./restore_keystone.sh <backup_file.sql.gz>

set -euo pipefail

BACKUP_FILE="${1:-}"

if [ -z "${BACKUP_FILE}" ]; then
    echo "Usage: $0 <backup_file.sql.gz>"
    echo "Available backups:"
    ls -la ./backups/keystone/keystone_backup_*.sql.gz 2>/dev/null || echo "No backups found"
    exit 1
fi

if [ ! -f "${BACKUP_FILE}" ]; then
    echo "ERROR: Backup file not found: ${BACKUP_FILE}"
    exit 1
fi

echo "Starting Keystone restore at $(date)"
echo "Backup file: ${BACKUP_FILE}"

# Check if keystone container is running
if ! docker compose -f ../docker-compose.yml ps keystone | grep -q "Up"; then
    echo "ERROR: Keystone container is not running"
    exit 1
fi

# Confirm restore
read -p "This will REPLACE the current database. Are you sure? (yes/no): " CONFIRM
if [ "${CONFIRM}" != "yes" ]; then
    echo "Restore cancelled"
    exit 0
fi

# Drop and recreate database
echo "Dropping and recreating database..."
docker compose -f ../docker-compose.yml exec -T keystone psql \
    -U "${POSTGRES_USER:-postgres}" \
    -d postgres \
    -c "DROP DATABASE IF EXISTS ${POSTGRES_DB:-tpt_soma}; CREATE DATABASE ${POSTGRES_DB:-tpt_soma};"

# Restore from backup
echo "Restoring database..."
gunzip -c "${BACKUP_FILE}" | docker compose -f ../docker-compose.yml exec -T keystone psql \
    -U "${POSTGRES_USER:-postgres}" \
    -d "${POSTGRES_DB:-tpt_soma}"

if [ $? -eq 0 ]; then
    echo "Restore completed successfully"
else
    echo "ERROR: Restore failed"
    exit 1
fi

# Run migrations to ensure schema is up to date
echo "Running migrations..."
docker compose -f ../docker-compose.yml exec -T api cargo run --bin migrate 2>/dev/null || echo "Migration binary not available, skipping"

echo "Restore process completed at $(date)"