#!/bin/bash
# Restore script for MinIO bucket data
# Usage: ./restore_minio.sh <backup_file.tar.gz>

set -euo pipefail

BACKUP_FILE="${1:-}"

if [ -z "${BACKUP_FILE}" ]; then
    echo "Usage: $0 <backup_file.tar.gz>"
    echo "Available backups:"
    ls -la ./backups/minio/minio_backup_*.tar.gz 2>/dev/null || echo "No backups found"
    exit 1
fi

if [ ! -f "${BACKUP_FILE}" ]; then
    echo "ERROR: Backup file not found: ${BACKUP_FILE}"
    exit 1
fi

echo "Starting MinIO restore at $(date)"
echo "Backup file: ${BACKUP_FILE}"

# Check if minio container is running
if ! docker compose -f ../docker-compose.yml ps minio | grep -q "Up"; then
    echo "ERROR: MinIO container is not running"
    exit 1
fi

# Confirm restore
read -p "This will REPLACE the current MinIO bucket data. Are you sure? (yes/no): " CONFIRM
if [ "${CONFIRM}" != "yes" ]; then
    echo "Restore cancelled"
    exit 0
fi

# Extract backup to temporary location
TEMP_DIR=$(mktemp -d)
echo "Extracting backup..."
tar -xzf "${BACKUP_FILE}" -C "${TEMP_DIR}"

# Find the extracted directory
EXTRACTED_DIR=$(find "${TEMP_DIR}" -maxdepth 1 -type d -name "minio_data_*" | head -1)
if [ -z "${EXTRACTED_DIR}" ]; then
    echo "ERROR: Could not find extracted data directory"
    rm -rf "${TEMP_DIR}"
    exit 1
fi

# Restore using mc mirror
echo "Restoring MinIO bucket..."
docker run --rm \
    --network tpt-soma_default \
    -v "${EXTRACTED_DIR}:/restore_data" \
    minio/mc:latest \
    sh -c "
        mc alias set local http://minio:9000 \${MINIO_ROOT_USER} \${MINIO_ROOT_PASSWORD} &&
        mc mirror --overwrite /restore_data local/tpt-soma
    "

if [ $? -eq 0 ]; then
    echo "Restore completed successfully"
else
    echo "ERROR: Restore failed"
    rm -rf "${TEMP_DIR}"
    exit 1
fi

# Cleanup
rm -rf "${TEMP_DIR}"
echo "Restore process completed at $(date)"