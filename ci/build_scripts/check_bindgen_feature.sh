#!/bin/bash

# ==============================================================================
# Validation script for rquickjs 'bindgen' feature configuration.
#
# PURPOSE:
# This script ensures that the conditional dependencies in Cargo.toml correctly
# toggle the 'bindgen' feature for rquickjs based on the target triple.
# We aim to disable 'bindgen' on platforms where pre-built bindings are 
# available to optimize build times and minimize external dependencies.
#
# METHODOLOGY:
# It uses 'cargo tree --target <TRIPLE>' to inspect the resolved dependency
# graph for each platform. The script compares the actual enabled features
# against two predefined lists:
#   1. TARGETS_NO_BINDGEN: Should use pre-compiled bindings (feature disabled).
#   2. TARGETS_WITH_BINDGEN: Requires local generation (feature enabled).
# ==============================================================================

# 1. Targets using pre-compiled bindings (Expected: 'bindgen' feature is OFF)
# Listed in https://github.com/DelSkayn/rquickjs
TARGETS_NO_BINDGEN=(
    "x86_64-unknown-linux-gnu"
    "i686-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "loongarch64-unknown-linux-gnu"
    "x86_64-unknown-linux-musl"             # our build target
    "aarch64-unknown-linux-musl"            # our build target
    "loongarch64-unknown-linux-musl"
    "x86_64-pc-windows-gnu"
    "i686-pc-windows-gnu"
    "x86_64-pc-windows-msvc"
    "aarch64-pc-windows-msvc"
    "x86_64-apple-darwin"                   # our build target
    "aarch64-apple-darwin"                  # our build target
    "wasm32-wasip1"
    "wasm32-wasip2"
)

# 2. Targets requiring local binding generation (Expected: 'bindgen' feature is ON)
TARGETS_WITH_BINDGEN=(
    "armv7-unknown-linux-musleabihf"
    "arm-unknown-linux-musleabihf"
    "arm-unknown-linux-musleabi"
    "armv5te-unknown-linux-musleabi"
    "riscv64gc-unknown-linux-musl"
    "i686-unknown-linux-musl"
)

errors=0

check_target() {
    local target=$1
    local expect_bindgen=$2
    
    # Extract the rquickjs line from the dependency tree
    local output
    output=$(cargo tree --target "$target" -f "{p} {f}" 2>/dev/null | grep "rquickjs v" | head -n 1)
    
    if [[ -z "$output" ]]; then
        echo "[ERROR] $target: rquickjs not found"
        ((errors++))
        return
    fi

    local has_bindgen=false
    if echo "$output" | grep -q "bindgen"; then
        has_bindgen=true
    fi

    if [ "$expect_bindgen" = true ]; then
        if [ "$has_bindgen" = true ]; then
            echo "[PASS] $target (bindgen included)"
        else
            echo "[FAIL] $target: Expected bindgen, but missing"
            ((errors++))
        fi
    else
        if [ "$has_bindgen" = false ]; then
            echo "[PASS] $target (bindgen excluded)"
        else
            echo "[FAIL] $target: Expected no bindgen, but found"
            ((errors++))
        fi
    fi
}

echo "Checking Exclusion List (No bindgen)..."
for t in "${TARGETS_NO_BINDGEN[@]}"; do
    check_target "$t" false
done

echo ""
echo "Checking Inclusion List (With bindgen)..."
for t in "${TARGETS_WITH_BINDGEN[@]}"; do
    check_target "$t" true
done

echo "---"
if [ $errors -eq 0 ]; then
    echo "All checks passed."
    exit 0
else
    echo "Summary: $errors error(s) found."
    exit 1
fi
