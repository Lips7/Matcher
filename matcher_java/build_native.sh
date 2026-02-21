#!/bin/bash

# Exit immediately if a command exits with a non-zero status.
set -e

echo "Building native Rust library (matcher_c)..."

# Move up to the project root
cd ..
cargo build --release --manifest-path=matcher_c/Cargo.toml

# Determine OS and Architecture to map to JNA's expected path format
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

JNA_OS=""
case "$OS" in
    "linux")
        JNA_OS="linux"
        ;;
    "darwin")
        JNA_OS="darwin"
        ;;
    *"mingw"* | *"msys"* | *"cygwin"*)
        JNA_OS="win32"
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

JNA_ARCH=""
case "$ARCH" in
    "x86_64" | "amd64")
        JNA_ARCH="x86-64"
        ;;
    "aarch64" | "arm64")
        JNA_ARCH="aarch64"
        ;;
    "i386" | "i686")
        JNA_ARCH="x86"
        ;;
    *)
        echo "Unsupported Architecture: $ARCH"
        exit 1
        ;;
esac

JNA_PATH="${JNA_OS}-${JNA_ARCH}"
echo "Detected JNA Path: $JNA_PATH"

# Create destination directory in the Java project resources
RESOURCES_DIR="matcher_java/src/main/resources/$JNA_PATH"
mkdir -p "$RESOURCES_DIR"

# Copy the corresponding shared library from Rust output
if [ "$JNA_OS" = "darwin" ]; then
    cp target/release/libmatcher_c.dylib "$RESOURCES_DIR/"
    echo "Copied libmatcher_c.dylib to $RESOURCES_DIR"
elif [ "$JNA_OS" = "linux" ]; then
    cp target/release/libmatcher_c.so "$RESOURCES_DIR/"
    echo "Copied libmatcher_c.so to $RESOURCES_DIR"
elif [ "$JNA_OS" = "win32" ]; then
    cp target/release/matcher_c.dll "$RESOURCES_DIR/"
    echo "Copied matcher_c.dll to $RESOURCES_DIR"
fi

echo "Native build and packaging complete."
