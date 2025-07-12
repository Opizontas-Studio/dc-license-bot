#!/bin/bash

# Simple local systemd deployment script

set -e

PROJECT_PATH="$(pwd)"
SERVICE_NAME="dc-bot"
USER="$USER"

echo "Building dc-bot..."
cargo build --release

echo "Stopping existing service..."
sudo systemctl stop $SERVICE_NAME 2>/dev/null || true

echo "Creating systemd service..."
sudo tee /etc/systemd/system/$SERVICE_NAME.service > /dev/null <<EOF
[Unit]
Description=Discord Bot Service
After=network.target

[Service]
Type=simple
User=$USER
WorkingDirectory=$PROJECT_PATH
ExecStart=$PROJECT_PATH/target/release/dc-bot -c config.toml -d ./data/bot.db -l system_licenses.json
Restart=always
RestartSec=10
Environment="RUST_LOG=info"

[Install]
WantedBy=multi-user.target
EOF

echo "Reloading systemd and starting service..."
sudo systemctl daemon-reload
sudo systemctl enable $SERVICE_NAME
sudo systemctl start $SERVICE_NAME

echo "Service started! Check status with:"
echo "  sudo systemctl status $SERVICE_NAME"
echo "  sudo journalctl -u $SERVICE_NAME -f"