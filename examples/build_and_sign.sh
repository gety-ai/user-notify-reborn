#!/bin/bash

# Build and Sign Script for User-Notify Basic Example (Ad-Hoc Signing)
# Usage: ./build_and_sign.sh [--no-sign]

set -e

SKIP_SIGNING=false

EXAMPLE_NAME="basic"
BUNDLE_ID="com.example.user-notify-reborn"

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

echo "🚀 Building and packaging user-notify basic example as macOS app bundle..."
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

echo "📦 Building $EXAMPLE_NAME..."

# Build the example from the root directory
cargo build --release --example "$EXAMPLE_NAME"

# Create app bundle
create_app_bundle "$EXAMPLE_NAME" "$BUNDLE_ID"

app_path="target/release/${EXAMPLE_NAME}.app"

if [ "$SKIP_SIGNING" = false ]; then
    echo "✍️ Ad-hoc signing $EXAMPLE_NAME app bundle..."
    # Ad-hoc sign the entire app bundle
    codesign --force --deep --sign "-" "$app_path"

    # Verify the signature
    echo "🔍 Verifying ad-hoc signature for $EXAMPLE_NAME..."
    if codesign --verify --deep --verbose "$app_path" 2>/dev/null; then
        echo "✅ $EXAMPLE_NAME app bundle ad-hoc signed and verified successfully"
    else
        echo "❌ Failed to verify ad-hoc signature for $EXAMPLE_NAME"
        exit 1
    fi
else
    echo "⚠️ Skipping signing for $EXAMPLE_NAME (--no-sign flag used)"
fi

echo "📍 App bundle location: $(pwd)/$app_path"
echo "📍 Bundle ID: $BUNDLE_ID"
echo

echo "🎉 Example built and packaged successfully!"

if [ "$SKIP_SIGNING" = false ]; then
    echo "✅ App bundle ad-hoc signed (local development signing)"
    echo
    echo "🔧 To run the signed app bundle:"
    echo "   open $app_path"
    echo "   # or double-click the .app file in Finder"
    echo
    echo "🔧 Alternative command line execution:"
    echo "   $app_path/Contents/MacOS/$EXAMPLE_NAME"
    echo
    echo "ℹ️ Ad-hoc signatures are valid for local execution but cannot be distributed"
else
    echo "💡 To build with ad-hoc signing, run:"
    echo "   ./examples/build_and_sign.sh"
    echo
    echo "🔧 To run the unsigned app bundle:"
    echo "   open $app_path"
fi

echo
echo "💡 App bundles with Info.plist are required for macOS notifications to work properly"
