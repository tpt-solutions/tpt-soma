#!/bin/bash
# Backup script for Keystone database (tpt-data volume)
# Usage: ./backup_keystone.sh [backup_dir]

set -euo pipefail

BACKUP_DIR="${1:-./backups/keystone}"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
BACKUP_FILE="${BACKUP_DIR}/keystone_backup_${TIMESTAMP}.sql.gz"

# Create backup directory
mkdir -p "${BACKUP_DIR}"

echo "Starting Keystone backup at $(date)"
echo "Backup file: ${BACKUP_FILE}"

# Check if keystone container is running
if ! docker compose -f ../docker-compose.yml ps keystone | grep -q "Up"; then
    echo "ERROR: Keystone container is not running"
    exit 1
fi

# Perform backup using pg_dump
docker compose -f ../docker-compose.yml exec -T keystone pg_dump \
    -U "${POSTGRES_USER:-postgres}" \
    -d "${POSTGRES_DB:-tpt_soma}" \
    --no-owner --no-privileges \
    | gzip > "${BACKUP_FILE}"

if [ $? -eq 0 ]; then
    echo "Backup completed successfully: ${BACKUP_FILE}"
    echo "Backup size: $(du -h "${BACKUP_FILE}" | cut -f1)"
else
    echo "ERROR: Backup failed"
    rm -f "${BACKUP_FILE}"
    exit 1
fi

# Cleanup old backups (keep last 7 days)
find "${BACKUP_DIR}" -name "keystone_backup_*.sql.gz" -mtime +7 -delete
echo "Old backups cleaned up (kept last 7 days)"