#!/bin/bash
# Setup Redis using Docker for the Drasi Platform example

set -e

echo "==================================="
echo "Drasi Platform Example - Redis Setup"
echo "==================================="
echo ""

# Check if Docker is installed
if ! command -v docker &> /dev/null; then
    echo "ERROR: Docker is not installed or not in PATH"
    echo ""
    echo "Please install Docker:"
    echo "  - macOS: https://docs.docker.com/desktop/install/mac-install/"
    echo "  - Linux: https://docs.docker.com/engine/install/"
    echo "  - Windows: https://docs.docker.com/desktop/install/windows-install/"
    exit 1
fi

# Check if Redis container already exists
if docker ps -a --format '{{.Names}}' | grep -q "^drasi-redis$"; then
    echo "Redis container 'drasi-redis' already exists"

    # Check if it's running
    if docker ps --format '{{.Names}}' | grep -q "^drasi-redis$"; then
        echo "Redis is already running on port 6379"
        echo ""
        echo "To view logs: docker logs drasi-redis"
        echo "To stop: docker stop drasi-redis"
    else
        echo "Starting existing Redis container..."
        docker start drasi-redis
        echo "Redis started on port 6379"
    fi
else
    echo "Starting Redis container..."
    docker run -d \
        --name drasi-redis \
        -p 6379:6379 \
        redis:7-alpine

    echo ""
    echo "✓ Redis started successfully on port 6379"
    echo ""
    echo "Container name: drasi-redis"
    echo "Port: 6379"
    echo ""
    echo "Useful commands:"
    echo "  View logs:    docker logs drasi-redis"
    echo "  Stop Redis:   docker stop drasi-redis"
    echo "  Remove Redis: docker rm drasi-redis"
    echo "  Redis CLI:    docker exec -it drasi-redis redis-cli"
fi

echo ""
echo "Redis is ready for the Drasi Platform example!"
