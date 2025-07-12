#!/bin/bash

# Local deployment script for dc-bot
# Creates systemd service on local machine

set -e

# Configuration
BINARY_NAME="dc-bot"
SERVICE_NAME="dc-license-bot"
CURRENT_USER=$(whoami)
CURRENT_DIR=$(pwd)
BINARY_PATH="$CURRENT_DIR/target/release/$BINARY_NAME"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

print_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if config.toml exists
if [ ! -f "config.toml" ]; then
    print_error "config.toml not found! Please create it first:"
    echo "cp config.example.toml config.toml"
    echo "# Then edit config.toml with your Discord token"
    exit 1
fi

print_step "Building dc-bot in release mode..."
cargo build --release

print_success "Build completed"

print_step "Creating systemd service..."

sudo tee /etc/systemd/system/$SERVICE_NAME.service > /dev/null <<EOF
[Unit]
Description=Discord License Bot - dc-license-bot
After=network.target

[Service]
Type=simple
User=$CURRENT_USER
WorkingDirectory=$CURRENT_DIR
ExecStart=$BINARY_PATH -c config.toml -d ./data/bot.db -l system_licenses.json
Restart=always
RestartSec=10
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
EOF

print_step "Enabling and starting service..."
sudo systemctl daemon-reload
sudo systemctl enable $SERVICE_NAME
sudo systemctl start $SERVICE_NAME

sleep 2

if sudo systemctl is-active --quiet $SERVICE_NAME; then
    print_success "dc-license-bot is running!"
    echo
    echo "Commands:"
    echo "  Logs: sudo journalctl -u $SERVICE_NAME -f"
    echo "  Stop: sudo systemctl stop $SERVICE_NAME"
    echo "  Restart: sudo systemctl restart $SERVICE_NAME"
else
    print_error "Service failed to start"
    echo "Check logs: sudo journalctl -u $SERVICE_NAME -n 20"
    exit 1
fi