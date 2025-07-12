#!/bin/bash

# Local deployment script for dc-bot
# Builds and runs as systemd service on current machine

set -e # Exit on any error

# Configuration
BINARY_NAME="dc-bot"
SERVICE_NAME="dc-bot"
SERVICE_USER="$USER"
DEPLOY_PATH="$HOME/dc-bot"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

print_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

print_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

print_error() {
    echo -e "${RED}[ERROR]${NC} $1"
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

print_success "Binary found at $BINARY_PATH"
print_success "Migration binary found at $MIGRATION_BINARY_PATH"

# Step 2: Stop existing systemd service if running
print_step "Stopping existing systemd service '$SERVICE_NAME'..."
if sudo systemctl stop $SERVICE_NAME 2>/dev/null; then
    print_success "Existing service stopped"
else
    echo "Service was not running or doesn't exist yet"
fi

# Step 3: Upload binaries to deployment path
print_step "Setting up deployment directory at $DEPLOY_PATH..."
mkdir -p "$DEPLOY_PATH"
cp "$BINARY_PATH" "$MIGRATION_BINARY_PATH" "$DEPLOY_PATH/"
chmod +x "$DEPLOY_PATH/$BINARY_NAME" "$DEPLOY_PATH/migration"
print_success "Binaries deployed to $DEPLOY_PATH"

# Step 4: Copy config files
if [ -f "config.toml" ]; then
    cp "config.toml" "$DEPLOY_PATH/"
    print_success "Config copied"
fi

if [ -f "system_licenses.json" ]; then
    cp "system_licenses.json" "$DEPLOY_PATH/"
    print_success "System licenses copied"
fi

# Step 5: Run migration
print_step "Running database migration..."
cd "$DEPLOY_PATH"
if ./migration; then
    print_success "Database migration completed"
else
    print_error "Database migration failed"
    exit 1
fi
cd - > /dev/null

# Step 6: Create systemd service file
print_step "Creating systemd service file..."
SERVICE_FILE_CONTENT="[Unit]
Description=Discord Bot Service
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=$SERVICE_USER
ExecStart=${DEPLOY_PATH}/${BINARY_NAME} -c ${DEPLOY_PATH}/config.toml -d ${DEPLOY_PATH}/bot.db -l ${DEPLOY_PATH}/system_licenses.json
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal
SyslogIdentifier=$SERVICE_NAME

# Environment variables
Environment="RUST_LOG=info"
Environment="RUST_BACKTRACE=1"

# Security settings
NoNewPrivileges=true
PrivateTmp=true
ProtectSystem=strict
ProtectHome=read-only
ReadWritePaths=${DEPLOY_PATH}

[Install]
WantedBy=multi-user.target"

# Upload service file
echo "$SERVICE_FILE_CONTENT" | sudo tee /etc/systemd/system/${SERVICE_NAME}.service > /dev/null
print_success "Service file created"

# Step 7: Reload systemd and enable service
print_step "Reloading systemd daemon..."
sudo systemctl daemon-reload
print_success "Systemd daemon reloaded"

print_step "Enabling service to start on boot..."
sudo systemctl enable $SERVICE_NAME
print_success "Service enabled for auto-start"

# Step 8: Start the service
print_step "Starting service '$SERVICE_NAME'..."
if sudo systemctl start $SERVICE_NAME; then
    print_success "Service started"
else
    print_error "Failed to start service"
    sudo journalctl -u $SERVICE_NAME --no-pager -n 50
    exit 1
fi

# Step 9: Verify the service is running
print_step "Verifying service status..."
sleep 3

SERVICE_STATUS=$(sudo systemctl is-active $SERVICE_NAME 2>/dev/null || echo "failed")

if [ "$SERVICE_STATUS" = "active" ]; then
    print_success "Service '$SERVICE_NAME' is running successfully"
    
    # Show service status
    print_step "Service status:"
    sudo systemctl status $SERVICE_NAME --no-pager -l
    
    echo ""
    echo -e "${GREEN}Deployment completed successfully!${NC}"
    echo ""
    echo -e "${YELLOW}Useful commands:${NC}"
    echo -e "View status:      ${BLUE}sudo systemctl status $SERVICE_NAME${NC}"
    echo -e "View logs:        ${BLUE}sudo journalctl -u $SERVICE_NAME -f --output cat${NC}"
    echo -e "Stop service:     ${BLUE}sudo systemctl stop $SERVICE_NAME${NC}"
    echo -e "Start service:    ${BLUE}sudo systemctl start $SERVICE_NAME${NC}"
    echo -e "Restart service:  ${BLUE}sudo systemctl restart $SERVICE_NAME${NC}"
    echo ""
    echo -e "${YELLOW}Migration commands:${NC}"
    echo -e "Run migration:    ${BLUE}cd $DEPLOY_PATH && ./migration${NC}"
else
    print_error "Service failed to start or is not running"
    print_step "Checking service logs for errors..."
    sudo journalctl -u $SERVICE_NAME --no-pager -l
    exit 1
fi