#!/bin/bash
# Cleanup script for Drasi Platform example
# Stops the Drasi server and removes Redis container

echo "==================================="
echo "Drasi Platform Example - Cleanup"
echo "==================================="
echo ""

# Stop any running drasi-server processes
echo "Stopping Drasi server processes..."
if pgrep -f "drasi-server.*drasi-platform" > /dev/null; then
    pkill -f "drasi-server.*drasi-platform"
    echo "✓ Drasi server stopped"
else
    echo "No Drasi server processes found"
fi

echo ""

# Stop and remove Redis container
echo "Stopping Redis container..."
if docker ps -a --format '{{.Names}}' | grep -q "^drasi-redis$"; then
    # Stop the container if running
    if docker ps --format '{{.Names}}' | grep -q "^drasi-redis$"; then
        docker stop drasi-redis > /dev/null 2>&1
        echo "✓ Redis container stopped"
    else
        echo "Redis container was already stopped"
    fi

    # Remove the container
    docker rm drasi-redis > /dev/null 2>&1
    echo "✓ Redis container removed"
else
    echo "No Redis container found"
fi

echo ""
echo "Cleanup complete!"
echo ""
echo "To start again:"
echo "  1. ./examples/drasi-platform/scripts/setup-redis.sh"
echo "  2. ./examples/drasi-platform/scripts/start-server.sh"
