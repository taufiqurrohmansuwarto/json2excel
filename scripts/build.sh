#!/bin/bash

# Build script untuk Excel Service

echo "🦀 Building Excel Service..."

# Build Docker image
echo "📦 Building Docker image..."
docker build -t excel-service:latest .

if [ $? -eq 0 ]; then
    echo "✅ Docker image built successfully"
else
    echo "❌ Docker build failed"
    exit 1
fi

# Run container
echo "🚀 Starting Excel Service..."
docker-compose up -d

if [ $? -eq 0 ]; then
    echo "✅ Excel Service started successfully"
    echo "🌐 Service available at: http://localhost:3333"
    echo ""
    echo "📋 Available endpoints:"
    echo "   GET  http://localhost:3333/health"
    echo "   GET  http://localhost:3333/test"
    echo "   GET  http://localhost:3333/status"
    echo "   POST http://localhost:3333/generate-excel"
    echo ""
    echo "🔍 Check logs: docker-compose logs -f excel-service"
    echo "🛑 Stop service: docker-compose down"
else
    echo "❌ Failed to start Excel Service"
    exit 1
fi