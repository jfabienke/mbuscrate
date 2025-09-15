#!/bin/bash
#
# RTT + defmt Logging Setup for Raspberry Pi 4/5
# This script sets up probe-rs for live log streaming from Pi via SWD/SWO
#

set -e

echo "=== RTT + defmt Logging Setup for Raspberry Pi 4/5 ==="
echo

# Check if we're on a supported platform
PLATFORM=$(uname -m)
OS=$(uname -s)

echo "Platform: $OS $PLATFORM"

# Install probe-rs if not already installed
if ! command -v probe-rs &> /dev/null; then
    echo "Installing probe-rs..."
    if [[ "$OS" == "Linux" ]]; then
        # Pi deployment
        curl --proto '=https' --tlsv1.2 -LsSf https://github.com/probe-rs/probe-rs/releases/latest/download/probe-rs-installer.sh | sh
    elif [[ "$OS" == "Darwin" ]]; then
        # macOS development
        brew install probe-rs
    else
        echo "Please install probe-rs manually: https://probe.rs/docs/getting-started/installation/"
        exit 1
    fi
else
    echo "probe-rs already installed: $(probe-rs --version)"
fi

# Check for probe-rs targets
echo
echo "Available probe-rs targets:"
probe-rs list targets | grep -i "bcm\|arm\|cortex" | head -10

# Detect if we're on a Pi
PI_MODEL=""
if [[ -f /proc/device-tree/model ]]; then
    PI_MODEL=$(cat /proc/device-tree/model)
    echo "Detected Pi model: $PI_MODEL"
fi

# Check GPIO permissions (Pi only)
if [[ "$PLATFORM" == "aarch64" || "$PLATFORM" == "armv7l" ]]; then
    echo
    echo "Checking GPIO permissions..."

    # Check if user is in gpio group
    if groups | grep -q gpio; then
        echo "✓ User is in gpio group"
    else
        echo "⚠ Adding user to gpio group (requires sudo)..."
        sudo usermod -a -G gpio $USER
        echo "  Please log out and back in for group changes to take effect"
    fi

    # Check GPIO sysfs access
    if [[ -w /sys/class/gpio/export ]]; then
        echo "✓ GPIO sysfs is writable"
    else
        echo "⚠ GPIO sysfs requires permissions setup"
    fi
fi

# Setup defmt dependencies
echo
echo "Setting up defmt logging dependencies..."

# Create defmt.toml for consistent formatting
cat > defmt.toml << 'EOF'
[default]
# Display format for RTT logs
display = "{t} {L} {s}"

[layouts]
# Time format: microseconds since boot
time = "{t:us}"

# Compact binary format for high-performance logging
compact = "{L:u8}{s:cstr}"
EOF

echo "✓ Created defmt.toml"

# Setup SWD pin configuration (Pi only)
if [[ "$PI_MODEL" == *"Raspberry Pi"* ]]; then
    echo
    echo "Setting up SWD GPIO pins..."

    # Enable SWD pins (requires gpio utility or device tree overlay)
    if command -v gpio &> /dev/null; then
        # Using wiringPi gpio utility
        gpio mode 22 out  # SWDIO (bidirectional, set as output initially)
        gpio mode 27 out  # SWDCLK
        gpio mode 24 out  # SWO
        echo "✓ Configured SWD pins using gpio utility"
    else
        echo "⚠ gpio utility not found. SWD pins may need manual configuration."
        echo "  Install wiringPi: sudo apt install wiringpi"
        echo "  Or configure via device tree overlay"
    fi

    # Show pin mapping
    echo
    echo "SWD Pin Connections for Raspberry Pi:"
    echo "  GPIO 22 (Pin 15) -> SWDIO"
    echo "  GPIO 27 (Pin 13) -> SWDCLK"
    echo "  GPIO 24 (Pin 18) -> SWO"
    echo "  GND (Pin 6/9/14/20/25/30/34/39) -> GND"
fi

# Create RTT monitoring script
echo
echo "Creating RTT monitoring script..."

cat > rtt-monitor.sh << 'EOF'
#!/bin/bash
#
# RTT Log Monitor - Live streaming of structured logs
#

set -e

echo "=== RTT Log Monitor ==="
echo "Connecting to Pi RTT logging via probe-rs..."
echo

# Check if target is accessible
if ! probe-rs list connected-devices &> /dev/null; then
    echo "❌ No probe devices found. Check SWD connections."
    exit 1
fi

echo "✓ Probe device detected"

# Start RTT logging with defmt decoding
echo "Starting live RTT log stream..."
echo "Press Ctrl+C to stop"
echo

# Use probe-rs rtt for live streaming
probe-rs rtt \
    --chip BCM2711 \
    --rtt-scan-memory \
    --defmt \
    --show-timestamps \
    --channel 0,1,2

EOF

chmod +x rtt-monitor.sh
echo "✓ Created rtt-monitor.sh"

# Create test script
echo
echo "Creating RTT test script..."

cat > test-rtt-logging.sh << 'EOF'
#!/bin/bash
#
# Test RTT + defmt logging implementation
#

set -e

echo "=== Testing RTT + defmt Logging ==="
echo

# Build with RTT feature
echo "Building with RTT logging feature..."
cargo build --features rtt-logging

# Run RTT logging tests
echo
echo "Running RTT logging tests..."
cargo test --features rtt-logging rtt_logging_tests -- --nocapture

# Run LoRa enhancement tests with RTT
echo
echo "Running LoRa enhancement tests..."
cargo test --features rtt-logging test_structured -- --nocapture

echo
echo "✓ All RTT tests completed"
echo
echo "To monitor live RTT logs:"
echo "  1. Ensure SWD connections are made"
echo "  2. Run: ./rtt-monitor.sh"
echo "  3. In another terminal: cargo run --features rtt-logging"

EOF

chmod +x test-rtt-logging.sh
echo "✓ Created test-rtt-logging.sh"

# Final summary
echo
echo "=== Setup Complete ==="
echo
echo "RTT + defmt logging is now configured!"
echo
echo "Next steps:"
echo "1. Connect SWD probe to Pi GPIO pins (see pin mapping above)"
echo "2. Test RTT logging: ./test-rtt-logging.sh"
echo "3. Monitor live logs: ./rtt-monitor.sh"
echo
echo "For device-config integration:"
echo "  cd ../device-config && cargo test --features rtt-logging"
echo
echo "Documentation:"
echo "  - RTT Guide: https://probe.rs/docs/tools/rtt/"
echo "  - defmt Book: https://defmt.ferrous-systems.com/"
echo "  - Pi GPIO: https://pinout.xyz/"