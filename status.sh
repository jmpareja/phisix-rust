#!/bin/bash
CONTAINER_NAME="phisix-api"

echo "=== Container Status ==="
podman ps -a --filter name="$CONTAINER_NAME"

echo ""
echo "=== Container Logs ==="
podman logs "$CONTAINER_NAME" --tail 20
