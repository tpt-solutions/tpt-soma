#!/bin/bash
# Backup script for MinIO bucket data
# Usage: ./backup_minio.sh [backup_dir]

set -euo pipefail

BACKUP_DIR="${1:-./backups/minio}"
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
BACKUP_FILE="${BACKUP_DIR}/minio_backup_${TIMESTAMP}.tar.gz"

# Create backup directory
mkdir -p "${BACKUP_DIR}"

echo "Starting MinIO backup at $(date)"
echo "Backup file: ${BACKUP_FILE}"

# Check if minio container is running
if ! docker compose -f ../docker-compose.yml ps minio | grep -q "Up"; then
    echo "ERROR: MinIO container is not running"
    exit 1
fi

# Create a temporary container to access minio data volume
# Using mc (MinIO Client) to mirror the bucket
docker run --rm \
    --network tpt-soma_default \
    -v "${BACKUP_DIR}:/backup" \
    minio/mc:latest \
    sh -c "
        mc alias set local http://minio:9000 \${MINIO_ROOT_USER} \${MINIO_ROOT_PASSWORD} &&
        mc mirror --overwrite local/tpt-soma /backup/minio_data_${TIMESTAMP} &&
        tar -czf /backup/$(basename ${BACKUP_FILE}) -C /backup minio_data_${TIMESTAMP} &&
        rm -rf /backup/minio_data_${TIMESTAMP}
    "

if [ $? -eq 0 ]; then
    echo "Backup completed successfully: ${BACKUP_FILE}"
    echo "Backup size: $(du -h "${BACKUP_FILE}" | cut -f1)"
else
    echo "ERROR: Backup failed"
    rm -f "${BACKUP_FILE}"
    exit 1
fi

# Cleanup old backups (keep last 7 days)
find "${BACKUP_DIR}" -name "minio_backup_*.tar.gz" -mtime +7 -delete
echo "Old backups cleaned up (kept last 7 days)"