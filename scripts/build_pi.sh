#!/bin/bash

# Build script for Raspberry Pi targets
# Usage: ./scripts/build_pi.sh [target] [mode]
# Targets: pi4-64, pi4-32, pi5, all
# Modes: debug, release (default: release)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
FEATURES="raspberry-pi"
MODE="${2:-release}"
TARGET="${1:-pi5}"

# Target mappings
declare -A TARGETS=(
    ["pi5"]="aarch64-unknown-linux-gnu"
    ["pi4-64"]="aarch64-unknown-linux-gnu" 
    ["pi4-32"]="armv7-unknown-linux-gnueabihf"
    ["pi3"]="armv7-unknown-linux-gnueabihf"
    ["pi2"]="armv7-unknown-linux-gnueabihf"
    ["pi1"]="arm-unknown-linux-gnueabihf"
    ["zero"]="arm-unknown-linux-gnueabihf"
)

print_usage() {
    echo "Usage: $0 [target] [mode]"
    echo ""
    echo "Targets:"
    for target in "${!TARGETS[@]}"; do
        echo "  $target -> ${TARGETS[$target]}"
    done
    echo "  all    -> Build for all targets"
    echo ""
    echo "Modes: debug, release (default: release)"
    echo ""
    echo "Examples:"
    echo "  $0 pi5 release      # Build for Pi 5 (release mode)"
    echo "  $0 pi4-64 debug     # Build for Pi 4 64-bit (debug mode)"  
    echo "  $0 all              # Build for all targets (release mode)"
}

check_cross() {
    if ! command -v cross &> /dev/null; then
        echo -e "${YELLOW}Warning: 'cross' not found. Install with: cargo install cross${NC}"
        echo -e "${BLUE}Falling back to regular cargo (requires toolchain setup)${NC}"
        return 1
    fi
    return 0
}

check_target() {
    local target_triple=$1
    if ! rustup target list --installed | grep -q "$target_triple"; then
        echo -e "${YELLOW}Installing target: $target_triple${NC}"
        rustup target add "$target_triple"
    fi
}

build_target() {
    local target_name=$1
    local target_triple=$2
    local build_mode=$3
    
    echo -e "${BLUE}Building for $target_name ($target_triple) in $build_mode mode...${NC}"
    
    local cargo_cmd="cargo"
    local build_args=""
    
    # Use cross if available, otherwise fallback to cargo
    if check_cross; then
        cargo_cmd="cross"
    else
        check_target "$target_triple"
    fi
    
    # Set build mode
    if [ "$build_mode" = "release" ]; then
        build_args="--release"
    fi
    
    # Build main library
    echo -e "${BLUE}  Building library...${NC}"
    $cargo_cmd build --target "$target_triple" --features "$FEATURES" $build_args
    
    # Build examples
    echo -e "${BLUE}  Building examples...${NC}"
    $cargo_cmd build --target "$target_triple" --features "$FEATURES" --examples $build_args
    
    # Check if build was successful
    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✅ Build successful for $target_name${NC}"
        
        # Show output directory
        local output_dir="target/$target_triple/$build_mode"
        echo -e "${BLUE}  Binaries located in: $output_dir${NC}"
        
        # List example binaries
        if [ -d "$output_dir/examples" ]; then
            echo -e "${BLUE}  Available examples:${NC}"
            ls -1 "$output_dir/examples" | sed 's/^/    /'
        fi
        
        echo ""
    else
        echo -e "${RED}❌ Build failed for $target_name${NC}"
        return 1
    fi
}

copy_to_pi() {
    local target_triple=$1
    local build_mode=$2
    local pi_host="${PI_HOST:-raspberrypi.local}"
    local pi_user="${PI_USER:-pi}"
    
    if [ -z "$PI_HOST" ]; then
        echo -e "${YELLOW}Tip: Set PI_HOST environment variable to auto-copy binaries${NC}"
        echo -e "${YELLOW}     e.g., export PI_HOST=raspberrypi.local${NC}"
        return 0
    fi
    
    echo -e "${BLUE}Copying binaries to $pi_user@$pi_host...${NC}"
    
    local output_dir="target/$target_triple/$build_mode"
    
    # Copy main examples
    if [ -f "$output_dir/examples/raspberry_pi_wmbus" ]; then
        scp "$output_dir/examples/raspberry_pi_wmbus" "$pi_user@$pi_host:~/" || true
    fi
    
    if [ -f "$output_dir/examples/pi_quick_start" ]; then
        scp "$output_dir/examples/pi_quick_start" "$pi_user@$pi_host:~/" || true
    fi
    
    echo -e "${GREEN}✅ Copy completed${NC}"
}

package_release() {
    local target_name=$1
    local target_triple=$2
    local build_mode=$3
    
    if [ "$build_mode" != "release" ]; then
        return 0
    fi
    
    local output_dir="target/$target_triple/$build_mode"
    local package_name="mbus-rs-$target_name-$(date +%Y%m%d)"
    local package_dir="releases/$package_name"
    
    echo -e "${BLUE}Creating release package for $target_name...${NC}"
    
    mkdir -p "$package_dir"
    
    # Copy binaries
    if [ -d "$output_dir/examples" ]; then
        cp -r "$output_dir/examples" "$package_dir/"
    fi
    
    # Copy documentation
    cp README.md "$package_dir/" 2>/dev/null || true
    cp docs/RASPBERRY_PI_SETUP.md "$package_dir/" 2>/dev/null || true
    
    # Create archive
    (cd releases && tar -czf "$package_name.tar.gz" "$package_name")
    
    echo -e "${GREEN}✅ Package created: releases/$package_name.tar.gz${NC}"
}

main() {
    # Check for help
    if [ "$1" = "-h" ] || [ "$1" = "--help" ]; then
        print_usage
        exit 0
    fi
    
    echo -e "${BLUE}mbus-rs Raspberry Pi Build Script${NC}"
    echo "=================================="
    
    # Validate target
    if [ "$TARGET" = "all" ]; then
        # Build for all targets
        for target_name in "${!TARGETS[@]}"; do
            build_target "$target_name" "${TARGETS[$target_name]}" "$MODE"
        done
    else
        if [ -z "${TARGETS[$TARGET]}" ]; then
            echo -e "${RED}Error: Unknown target '$TARGET'${NC}"
            echo ""
            print_usage
            exit 1
        fi
        
        build_target "$TARGET" "${TARGETS[$TARGET]}" "$MODE"
        
        # Optional: Copy to Pi if SSH configured
        copy_to_pi "${TARGETS[$TARGET]}" "$MODE"
        
        # Optional: Create release package
        package_release "$TARGET" "${TARGETS[$TARGET]}" "$MODE"
    fi
    
    echo -e "${GREEN}🎉 Build process completed!${NC}"
    echo ""
    echo -e "${BLUE}Next steps:${NC}"
    echo "1. Copy binary to your Raspberry Pi"
    echo "2. Enable SPI: Add 'dtparam=spi=on' to /boot/config.txt"  
    echo "3. Run with: sudo ./raspberry_pi_wmbus test"
    echo ""
    echo "For more details, see: docs/RASPBERRY_PI_SETUP.md"
}

# Run main function
main "$@"