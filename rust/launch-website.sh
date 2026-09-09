#!/bin/bash

# Exit on error
set -e

# Get the directory of the script
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"

echo "🚀 Starting build process for CloudLess Website..."

# 1. Build Tailwind CSS
echo "📦 Building Tailwind CSS..."
cd "$DIR/website"

if ! command -v npx &> /dev/null; then
    echo "❌ Error: 'npx' is not installed. Please install Node.js first."
    exit 1
fi

npx tailwindcss -i ./src/styles/input.css -o ./static/output.css --minify

# 2. Start API Server in background (skip if already running)
cd "$DIR"
if curl -s http://localhost:9000/health > /dev/null 2>&1; then
    echo "✅ API server already running on port 9000, skipping"
    API_PID=""
else
    echo "🔧 Starting API server on port 9000..."
    cargo run -p api_server &
    API_PID=$!

    # Wait for API server to be ready
    echo "⏳ Waiting for API server..."
    for i in $(seq 1 30); do
        if curl -s http://localhost:9000/health > /dev/null 2>&1; then
            echo "✅ API server is ready"
            break
        fi
        if ! kill -0 $API_PID 2>/dev/null; then
            echo "❌ API server failed to start"
            exit 1
        fi
        sleep 1
    done
fi

# 3. Run Website
echo "🖥️ Launching website on port 3000..."
if [ -n "$API_PID" ]; then
    trap "kill $API_PID 2>/dev/null" EXIT
fi
cargo run -p website
