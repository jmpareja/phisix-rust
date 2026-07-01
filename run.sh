#!/bin/bash
set -e

CONTAINER_NAME="phisix-api"
IMAGE_NAME="phisix-rust"
IMAGE_TAG="latest"
PORT=${PORT:-8080}

# Create a local directory for database persistence if it doesn't exist
DATA_DIR="$(pwd)/data"
mkdir -p "$DATA_DIR"

echo "Running Podman container ${CONTAINER_NAME}..."
echo "Exposing API at http://localhost:${PORT}"
echo "Persisting database at ${DATA_DIR}"

# Run the container in the background with volume mapping
# The :Z suffix is vital for SELinux labeling on systems running SELinux
podman run -d \
  --name "$CONTAINER_NAME" \
  -p "${PORT}:8080" \
  -v "${DATA_DIR}:/app/data:Z" \
  --restart always \
  "${IMAGE_NAME}:${IMAGE_TAG}"

echo "Container started successfully."
