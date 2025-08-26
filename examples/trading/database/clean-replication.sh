#!/bin/bash

# Copyright 2025 The Drasi Authors.
#
# Licensed under the Apache License, Version 2.0 (the "License");
# you may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#     http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Clean up PostgreSQL replication slot
# Use this if you encounter "replication slot already exists" errors

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "Cleaning up PostgreSQL replication slot..."

cd "$SCRIPT_DIR/database"

# Check if PostgreSQL is running
if docker-compose ps | grep -q "trading-postgres.*Up"; then
    echo "PostgreSQL is running. Dropping replication slot..."
    
    # Drop the replication slot if it exists
    docker-compose exec -T postgres psql -U postgres -d trading_demo -c \
        "SELECT pg_drop_replication_slot('drasi_trading_slot') WHERE EXISTS (SELECT 1 FROM pg_replication_slots WHERE slot_name = 'drasi_trading_slot');" 2>/dev/null || true
    
    echo "Replication slot cleaned up."
else
    echo "PostgreSQL is not running."
    echo "To completely reset, run:"
    echo "  docker-compose down -v"
    echo "This will remove all data and start fresh."
fi

echo "Done!"