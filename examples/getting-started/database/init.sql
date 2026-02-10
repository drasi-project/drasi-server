-- Copyright 2025 The Drasi Authors.
--
-- Licensed under the Apache License, Version 2.0 (the "License");
-- you may not use this file except in compliance with the License.
-- You may obtain a copy of the License at
--
--     http://www.apache.org/licenses/LICENSE-2.0
--
-- Unless required by applicable law or agreed to in writing, software
-- distributed under the License is distributed on an "AS IS" BASIS,
-- WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
-- See the License for the specific language governing permissions and
-- limitations under the License.

-- Getting Started Tutorial Database Schema
-- This schema mirrors the Drasi Platform getting-started tutorial

-- Create user with replication privileges for CDC
DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_user WHERE usename = 'drasi_user') THEN
        CREATE USER drasi_user WITH REPLICATION LOGIN PASSWORD 'drasi_password';
    END IF;
END
$$;

-- Grant permissions on the database
GRANT CREATE ON DATABASE getting_started TO drasi_user;
GRANT ALL PRIVILEGES ON DATABASE getting_started TO drasi_user;

-- Drop existing table if exists
DROP TABLE IF EXISTS message CASCADE;

-- Message table matching Platform tutorial schema
-- Stores messages with sender and content
CREATE TABLE message (
    messageid SERIAL PRIMARY KEY,
    "from" VARCHAR(50) NOT NULL,
    message VARCHAR(200) NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

-- Set REPLICA IDENTITY to FULL for complete CDC support
-- This ensures all columns are included in change events
ALTER TABLE message REPLICA IDENTITY FULL;

-- Ensure drasi_user owns the table
ALTER TABLE message OWNER TO drasi_user;

-- Grant permissions to drasi_user
GRANT USAGE ON SCHEMA public TO drasi_user;
GRANT SELECT ON ALL TABLES IN SCHEMA public TO drasi_user;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO drasi_user;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO drasi_user;

-- Create publication for logical replication
-- This publication includes only the message table
CREATE PUBLICATION drasi_pub FOR TABLE message;

-- Create replication slot for CDC
-- The slot uses pgoutput for logical decoding
SELECT pg_create_logical_replication_slot('drasi_slot', 'pgoutput');

-- Insert initial sample data (matching Platform tutorial)
INSERT INTO message ("from", message) VALUES
    ('Buzz Lightyear', 'To infinity and beyond!'),
    ('Brian Kernighan', 'Hello World'),
    ('Antoninus', 'I am Spartacus'),
    ('David', 'I am Spartacus');

-- Verify the setup
DO $$
BEGIN
    RAISE NOTICE 'Getting Started database initialized successfully!';
    RAISE NOTICE 'Tables: message';
    RAISE NOTICE 'Publication: drasi_pub';
    RAISE NOTICE 'Replication slot: drasi_slot';
END
$$;
