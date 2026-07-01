#!/bin/bash
CONTAINER_NAME="phisix-api"

echo "Stopping container ${CONTAINER_NAME}..."
podman stop "$CONTAINER_NAME" 2>/dev/null || true

echo "Removing container ${CONTAINER_NAME}..."
podman rm "$CONTAINER_NAME" 2>/dev/null || true

echo "Done."
