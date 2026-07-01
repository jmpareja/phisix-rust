#!/bin/bash
set -e

IMAGE_NAME="phisix-rust"
IMAGE_TAG="latest"

echo "Building Podman container image ${IMAGE_NAME}:${IMAGE_TAG}..."
podman build -t ${IMAGE_NAME}:${IMAGE_TAG} -f Containerfile .
echo "Build complete successfully."
