#!/bin/bash

# Build and Sign Script for All User-Notify Examples (Ad-Hoc Signing)
# Usage: ./build_and_sign.sh [--no-sign]

set -e

SKIP_SIGNING=false

BUNDLE_ID_PREFIX="com.example.user-notify-reborn"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
    --no-sign)
        SKIP_SIGNING=true
        shift
        ;;
    *)
        echo "Unknown option: $1"
        echo "Usage: $0 [--no-sign]"
        echo "  --no-sign: Skip code signing"
        exit 1
        ;;
    esac
done

# Function to discover all example files
discover_examples() {
    local examples=()
    for file in examples/*.rs; do
        if [ -f "$file" ]; then
            local basename=$(basename "$file" .rs)
            examples+=("$basename")
        fi
    done
    echo "${examples[@]}"
}

# Get all examples
EXAMPLES=($(discover_examples))

echo "🚀 Building and packaging all user-notify examples as macOS app bundles..."
echo "Found examples: ${EXAMPLES[*]}"
if [ "$SKIP_SIGNING" = true ]; then
    echo "Signing: Disabled (--no-sign flag used)"
else
    echo "Signing: Ad-Hoc (no Developer ID required)"
fi
echo

create_app_bundle() {
    local example_name="$1"
    local bundle_id="$2"
    local binary_path="target/release/examples/$example_name"
    local app_name="${example_name}.app"
    local app_path="target/release/$app_name"

    echo "📦 Creating app bundle for $example_name..."

    # Create app bundle structure
    mkdir -p "$app_path/Contents/MacOS"
    mkdir -p "$app_path/Contents/Resources"

    # Copy binary to bundle
    cp "$binary_path" "$app_path/Contents/MacOS/$example_name"

    # Create Info.plist from template
    sed -e "s/EXECUTABLE_NAME/$example_name/g" \
        -e "s/BUNDLE_ID/$bundle_id/g" \
        examples/Info.plist >"$app_path/Contents/Info.plist"

    echo "✅ App bundle created: $app_path"
    return 0
}

# Build all examples
echo "📦 Building all examples..."
for example in "${EXAMPLES[@]}"; do
    echo "Building example: $example"
    cargo build --release --example "$example"
done
echo

# Create app bundles for all examples
for example in "${EXAMPLES[@]}"; do
    bundle_id="${BUNDLE_ID_PREFIX}.${example}"
    create_app_bundle "$example" "$bundle_id"
done

# Sign all app bundles if signing is not skipped
if [ "$SKIP_SIGNING" = false ]; then
    echo "✍️ Ad-hoc signing all app bundles..."
    for example in "${EXAMPLES[@]}"; do
        app_path="target/release/${example}.app"
        echo "Signing $example..."
        # Ad-hoc sign the entire app bundle
        codesign --force --deep --sign "-" "$app_path"

        # Verify the signature
        echo "🔍 Verifying ad-hoc signature for $example..."
        if codesign --verify --deep --verbose "$app_path" 2>/dev/null; then
            echo "✅ $example app bundle ad-hoc signed and verified successfully"
        else
            echo "❌ Failed to verify ad-hoc signature for $example"
            exit 1
        fi
    done
else
    echo "⚠️ Skipping signing for all examples (--no-sign flag used)"
fi

echo "📍 App bundle locations:"
for example in "${EXAMPLES[@]}"; do
    app_path="target/release/${example}.app"
    bundle_id="${BUNDLE_ID_PREFIX}.${example}"
    echo "  - $(pwd)/$app_path (Bundle ID: $bundle_id)"
done
echo

echo "🎉 All examples built and packaged successfully!"

if [ "$SKIP_SIGNING" = false ]; then
    echo "✅ All app bundles ad-hoc signed (local development signing)"
    echo
    echo "🔧 To run the signed app bundles:"
    for example in "${EXAMPLES[@]}"; do
        app_path="target/release/${example}.app"
        echo "   open $app_path  # Run $example"
    done
    echo "   # or double-click the .app files in Finder"
    echo
    echo "🔧 Alternative command line execution:"
    for example in "${EXAMPLES[@]}"; do
        app_path="target/release/${example}.app"
        echo "   $app_path/Contents/MacOS/$example"
    done
    echo
    echo "ℹ️ Ad-hoc signatures are valid for local execution but cannot be distributed"
else
    echo "💡 To build with ad-hoc signing, run:"
    echo "   ./examples/build_and_sign.sh"
    echo
    echo "🔧 To run the unsigned app bundles:"
    for example in "${EXAMPLES[@]}"; do
        app_path="target/release/${example}.app"
        echo "   open $app_path  # Run $example"
    done
fi

echo
echo "💡 App bundles with Info.plist are required for macOS notifications to work properly"
