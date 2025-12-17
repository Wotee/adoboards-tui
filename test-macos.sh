#!/usr/bin/env bash
# macOS Test Setup for Adoboards Nix Configuration

set -euo pipefail

echo "🧪 Adoboards macOS Test Setup"
echo "=============================="
echo ""

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Test config directory
TEST_CONFIG_DIR="$HOME/Library/Application Support/adoboards"
TEST_CONFIG_FILE="$TEST_CONFIG_DIR/default-config.toml"

function print_step() {
    echo -e "${BLUE}▶${NC} $1"
}

function print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

function print_error() {
    echo -e "${RED}✗${NC} $1"
}

# Test 1: Flake Check
print_step "Test 1: Running flake check..."
if nix flake check 2>&1; then
    print_success "Flake check passed"
else
    print_error "Flake check failed"
    exit 1
fi
echo ""

# Test 2: Show flake outputs
print_step "Test 2: Showing flake outputs..."
nix flake show
echo ""

# Test 3: Build package
print_step "Test 3: Building package..."
if nix build; then
    print_success "Package built successfully"
else
    print_error "Build failed"
    exit 1
fi
echo ""

# Test 4: Enter dev shell and check tools
print_step "Test 4: Checking dev shell tools..."
if nix develop -c bash -c "which adoboards-generate-config" > /dev/null 2>&1; then
    print_success "Dev shell tools available"
else
    print_error "Dev shell tools missing"
    exit 1
fi
echo ""

# Test 5: Generate config with defaults
print_step "Test 5: Generating config with defaults..."
# Remove existing config if present
if [ -f "$TEST_CONFIG_FILE" ]; then
    echo "Backing up existing config..."
    mv "$TEST_CONFIG_FILE" "$TEST_CONFIG_FILE.backup"
fi

nix develop -c adoboards-generate-config
if [ -f "$TEST_CONFIG_FILE" ]; then
    print_success "Config file created at: $TEST_CONFIG_FILE"
    echo ""
    echo "Content:"
    cat "$TEST_CONFIG_FILE"
    echo ""
    # Verify structure
    if grep -q "\[common\]" "$TEST_CONFIG_FILE" && grep -q "\[keys\]" "$TEST_CONFIG_FILE" && grep -q "\[\[boards\]\]" "$TEST_CONFIG_FILE"; then
        print_success "Config has correct structure (common, keys, boards)"
    else
        print_error "Config missing required sections"
        exit 1
    fi
else
    print_error "Config file not created"
    exit 1
fi
echo ""

# Test 6: Generate config with custom values
print_step "Test 6: Generating config with custom values..."
nix develop -c adoboards-generate-config "Custom User" "testorg" "testproject" "Test Team"
if grep -q "testorg" "$TEST_CONFIG_FILE" && grep -q "Custom User" "$TEST_CONFIG_FILE"; then
    print_success "Custom values applied"
    echo ""
    echo "Content:"
    cat "$TEST_CONFIG_FILE"
    echo ""
else
    print_error "Custom values not applied"
    exit 1
fi
echo ""

# Test 7: Validate TOML syntax
print_step "Test 7: Validating TOML syntax..."
if command -v python3 &> /dev/null; then
    # Try tomllib (Python 3.11+) first, fallback to toml package
    if python3 -c "import sys; sys.exit(0 if sys.version_info >= (3, 11) else 1)" 2>/dev/null; then
        if python3 -c "import tomllib; tomllib.load(open('$TEST_CONFIG_FILE', 'rb'))" 2>&1; then
            print_success "TOML syntax valid (tomllib)"
        else
            print_error "Invalid TOML syntax"
            exit 1
        fi
    elif python3 -c "import toml" 2>/dev/null; then
        if python3 -c "import toml; toml.load(open('$TEST_CONFIG_FILE'))" 2>&1; then
            print_success "TOML syntax valid (toml)"
        else
            print_error "Invalid TOML syntax"
            exit 1
        fi
    else
        echo "⚠️  TOML validation library not available (need Python 3.11+ or 'pip install toml')"
        echo "    Skipping syntax validation..."
    fi
else
    echo "⚠️  Python3 not available, skipping TOML validation"
fi
echo ""

# Test 8: Test standalone config generator
print_step "Test 8: Testing standalone config generator..."
if nix run .#config-generator -- "Standalone User" "standalone-org" "standalone-proj" "Standalone Team"; then
    print_success "Standalone generator works"
    echo ""
    echo "Content:"
    cat "$TEST_CONFIG_FILE"
    echo ""
    # Verify me field
    if grep -q 'me = "Standalone User"' "$TEST_CONFIG_FILE"; then
        print_success "User 'me' field set correctly"
    else
        print_error "'me' field not set correctly"
        exit 1
    fi
else
    print_error "Standalone generator failed"
    exit 1
fi
echo ""

# Restore backup if exists
if [ -f "$TEST_CONFIG_FILE.backup" ]; then
    print_step "Restoring original config..."
    mv "$TEST_CONFIG_FILE.backup" "$TEST_CONFIG_FILE"
    print_success "Original config restored"
fi

echo ""
echo "=============================="
echo -e "${GREEN}✓ All tests passed!${NC}"
echo ""
echo "Next steps:"
echo "1. Test Home Manager module (see test-home-manager/)"
echo "2. Run the built application: ./result/bin/adoboards"
echo "3. Check full test plan: TEST-PLAN.md"
