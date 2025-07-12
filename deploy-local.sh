#!/bin/bash

# Local deployment script for dc-license-bot
# Installs as systemd service on the current machine

set -e # Exit on any error

# Configuration
BINARY_NAME="dc-bot"
SERVICE_NAME="dc-license-bot"
SERVICE_USER="$USER" # Use current user
DEPLOY_PATH="$HOME/dc-license-bot" # Deploy to user's home directory

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_step() {
    printf "${BLUE}[STEP]${NC} %s\n" "$1"
}

print_success() {
    printf "${GREEN}[SUCCESS]${NC} %s\n" "$1"
}

print_warning() {
    printf "${YELLOW}[WARNING]${NC} %s\n" "$1"
}

print_error() {
    printf "${RED}[ERROR]${NC} %s\n" "$1"
}

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    print_error "Cargo.toml not found. Please run this script from the project root directory."
    exit 1
fi

# Step 1: Build the binary locally
print_step "Building release binary..."
if cargo build --workspace --release; then
    print_success "Binary built successfully"
else
    print_error "Failed to build binary"
    exit 1
fi

# Strip binary to reduce size
print_step "Stripping binary to reduce size..."
strip target/release/${BINARY_NAME}
strip target/release/migration

# Check if binaries exist
BINARY_PATH="target/release/${BINARY_NAME}"
MIGRATION_BINARY_PATH="target/release/migration"
if [ ! -f "$BINARY_PATH" ]; then
    print_error "Binary not found at $BINARY_PATH"
    exit 1
fi

if [ ! -f "$MIGRATION_BINARY_PATH" ]; then
    print_error "Migration binary not found at $MIGRATION_BINARY_PATH"
    exit 1
fi

# Show binary size
BINARY_SIZE=$(du -h "$BINARY_PATH" | cut -f1)
print_success "Binary built: $BINARY_SIZE"

# Step 2: Stop existing systemd service if running
print_step "Checking for existing service..."
if systemctl is-active --quiet $SERVICE_NAME 2>/dev/null; then
    print_step "Stopping existing service..."
    sudo systemctl stop $SERVICE_NAME
    print_success "Service stopped"
fi

# Step 3: Create deployment directory
print_step "Setting up deployment directory at $DEPLOY_PATH..."
mkdir -p "$DEPLOY_PATH"

# Copy binaries (with different names to avoid conflicts with source directories)
cp "$BINARY_PATH" "$DEPLOY_PATH/$BINARY_NAME"
cp "$MIGRATION_BINARY_PATH" "$DEPLOY_PATH/migration-bin"
chmod +x "$DEPLOY_PATH/$BINARY_NAME" "$DEPLOY_PATH/migration-bin"

# Copy configuration files
if [ -f "config.toml" ]; then
    cp "config.toml" "$DEPLOY_PATH/"
    print_success "Copied config.toml"
else
    print_error "config.toml not found!"
    echo "Please create config.toml from config.example.toml and configure your Discord token"
    exit 1
fi

if [ -f "system_licenses.json" ]; then
    cp "system_licenses.json" "$DEPLOY_PATH/"
    print_success "Copied system_licenses.json"
else
    print_error "system_licenses.json not found!"
    exit 1
fi

# Create data directory
mkdir -p "$DEPLOY_PATH/data"

print_success "Files deployed to $DEPLOY_PATH"

# Step 4: Initialize and migrate database
print_step "Initializing database..."
cd "$DEPLOY_PATH"

# Run database migration
if ./migration-bin; then
    print_success "Database migration completed"
else
    print_error "Database migration failed"
    exit 1
fi
cd - > /dev/null

# Step 5: Create systemd service file
print_step "Creating systemd service file..."
SERVICE_FILE_CONTENT="[Unit]
Description=DC License Bot - Discord License Management Bot
Documentation=https://github.com/Opizontas-Studio/dc-license-bot
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=$SERVICE_USER
Group=$SERVICE_USER
WorkingDirectory=$DEPLOY_PATH

# Start command
ExecStart=$DEPLOY_PATH/$BINARY_NAME -c $DEPLOY_PATH/config.toml -d $DEPLOY_PATH/data/bot.db -l $DEPLOY_PATH/system_licenses.json

# Restart configuration
Restart=always
RestartSec=10
StartLimitInterval=600
StartLimitBurst=5

# Logging
StandardOutput=journal
StandardError=journal
SyslogIdentifier=$SERVICE_NAME

# Environment variables
Environment=\"RUST_LOG=info\"
Environment=\"RUST_BACKTRACE=1\"

# Security hardening
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=true
ReadWritePaths=$DEPLOY_PATH/data

# Resource limits (optional, adjust as needed)
# MemoryLimit=512M
# CPUQuota=50%

[Install]
WantedBy=multi-user.target"

# Write service file
echo "$SERVICE_FILE_CONTENT" | sudo tee /etc/systemd/system/${SERVICE_NAME}.service > /dev/null
print_success "Service file created at /etc/systemd/system/${SERVICE_NAME}.service"

# Step 6: Reload systemd and enable service
print_step "Configuring systemd..."
sudo systemctl daemon-reload
sudo systemctl enable $SERVICE_NAME
print_success "Service enabled for auto-start on boot"

# Step 7: Start the service
print_step "Starting service..."
if sudo systemctl start $SERVICE_NAME; then
    print_success "Service started successfully"
else
    print_error "Failed to start service"
    sudo journalctl -u $SERVICE_NAME --no-pager -n 50
    exit 1
fi

# Step 8: Verify service is running
sleep 2
if systemctl is-active --quiet $SERVICE_NAME; then
    print_success "Service is running!"
    
    echo ""
    sudo systemctl status $SERVICE_NAME --no-pager
    
    echo ""
    printf "${GREEN}═══════════════════════════════════════════════════════════${NC}\n"
    printf "${GREEN}Deployment completed successfully!${NC}\n"
    printf "${GREEN}═══════════════════════════════════════════════════════════${NC}\n"
    echo ""
    printf "${YELLOW}📋 Service Management Commands:${NC}\n"
    printf "  Status:  ${BLUE}sudo systemctl status $SERVICE_NAME${NC}\n"
    printf "  Logs:    ${BLUE}sudo journalctl -u $SERVICE_NAME -f${NC}\n"
    printf "  Restart: ${BLUE}sudo systemctl restart $SERVICE_NAME${NC}\n"
    printf "  Stop:    ${BLUE}sudo systemctl stop $SERVICE_NAME${NC}\n"
    echo ""
    printf "${YELLOW}🔄 Update Workflow:${NC}\n"
    printf "  1. ${BLUE}git pull${NC}                    # Pull latest changes\n"
    printf "  2. ${BLUE}./deploy-local.sh${NC}          # Redeploy\n"
    echo ""
    printf "${YELLOW}📁 Deployment Location:${NC}\n"
    printf "  ${BLUE}$DEPLOY_PATH${NC}\n"
    echo ""
else
    print_error "Service is not running!"
    echo "Check logs with: sudo journalctl -u $SERVICE_NAME -n 100"
    exit 1
fi