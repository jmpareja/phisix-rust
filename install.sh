#!/bin/bash
set -e

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SERVICE_NAME="phisix-api.service"
USER_SYSTEMD_DIR="${HOME}/.config/systemd/user"

echo "=== PHISIX API Installation Script ==="
echo "Project path: ${PROJECT_DIR}"

# 1. Build the container image
echo "Building Podman container image..."
cd "${PROJECT_DIR}"
./build.sh

# 2. Stop any standalone running container instance to release the port/name
echo "Cleaning up any running standalone container..."
./stop.sh || true

# 3. Create systemd user directory if it doesn't exist
mkdir -p "${USER_SYSTEMD_DIR}"

# 4. Copy systemd service file
echo "Installing systemd user service unit..."
cp "${PROJECT_DIR}/${SERVICE_NAME}" "${USER_SYSTEMD_DIR}/${SERVICE_NAME}"

# 5. Enable linger so user services start automatically at boot without login
echo "Ensuring user linger is enabled for $(whoami)..."
if command -v loginctl >/dev/null 2>&1; then
    sudo loginctl enable-linger "$(whoami)" || true
fi

# 6. Reload systemd user daemon and enable service
echo "Reloading systemd user daemon..."
systemctl --user daemon-reload

echo "Enabling and starting ${SERVICE_NAME}..."
systemctl --user enable --now "${SERVICE_NAME}"

echo ""
echo "=== Installation Completed Successfully ==="
echo "Status check: systemctl --user status ${SERVICE_NAME}"
echo "Logs: journalctl --user -u ${SERVICE_NAME} -f"
