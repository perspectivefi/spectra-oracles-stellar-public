#!/bin/bash
set -e

# Build and optimize Spectra Oracle contracts
#
# Usage:
#   ./scripts/build.sh [--skip-optimize]
#
# This script:
# 1. Builds contracts in release mode for wasm32v1-none
# 2. Optimizes WASM files using wasm-opt (requires binaryen)
# 3. Outputs optimized files to target/wasm32v1-none/optimized/

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

SKIP_OPTIMIZE=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-optimize)
            SKIP_OPTIMIZE=true
            shift
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

echo -e "${YELLOW}=== Building Spectra Oracle Contracts ===${NC}"

# Build contracts
echo -e "${YELLOW}Compiling contracts...${NC}"
cargo build --release --target wasm32v1-none

ORACLE_WASM="target/wasm32v1-none/release/spectra_oracle.wasm"
FACTORY_WASM="target/wasm32v1-none/release/spectra_oracle_factory.wasm"

# Verify WASM files exist
if [[ ! -f "$ORACLE_WASM" ]]; then
    echo -e "${RED}Error: Oracle WASM not found at $ORACLE_WASM${NC}"
    exit 1
fi

if [[ ! -f "$FACTORY_WASM" ]]; then
    echo -e "${RED}Error: Factory WASM not found at $FACTORY_WASM${NC}"
    exit 1
fi

echo -e "${GREEN}Build complete!${NC}"
echo "  Oracle:  $(ls -lh $ORACLE_WASM | awk '{print $5}')"
echo "  Factory: $(ls -lh $FACTORY_WASM | awk '{print $5}')"

if [[ "$SKIP_OPTIMIZE" == "true" ]]; then
    echo -e "${YELLOW}Skipping optimization (--skip-optimize)${NC}"
    exit 0
fi

# Check for wasm-opt
if ! command -v wasm-opt &> /dev/null; then
    echo -e "${YELLOW}wasm-opt not found. Install binaryen for optimization:${NC}"
    echo "  brew install binaryen"
    echo ""
    echo "Skipping optimization..."
    exit 0
fi

# Optimize WASM files
echo ""
echo -e "${YELLOW}Optimizing WASM files...${NC}"

mkdir -p target/wasm32v1-none/optimized

wasm-opt -Oz --strip-debug --strip-dwarf --strip-producers \
    "$ORACLE_WASM" \
    -o target/wasm32v1-none/optimized/spectra_oracle.wasm

wasm-opt -Oz --strip-debug --strip-dwarf --strip-producers \
    "$FACTORY_WASM" \
    -o target/wasm32v1-none/optimized/spectra_oracle_factory.wasm

ORACLE_OPT="target/wasm32v1-none/optimized/spectra_oracle.wasm"
FACTORY_OPT="target/wasm32v1-none/optimized/spectra_oracle_factory.wasm"

echo -e "${GREEN}Optimization complete!${NC}"
echo ""
echo "=== Final WASM Sizes ==="
echo "  Oracle:  $(ls -lh $ORACLE_OPT | awk '{print $5}') (optimized)"
echo "  Factory: $(ls -lh $FACTORY_OPT | awk '{print $5}') (optimized)"
echo ""
echo "Optimized files are in: target/wasm32v1-none/optimized/"
