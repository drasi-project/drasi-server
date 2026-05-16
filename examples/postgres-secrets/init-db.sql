-- init-db.sql  —  runs inside the PostgreSQL container on first start.

-- Create the demo database
CREATE DATABASE drasi_demo;

\c drasi_demo

-- Sample table: temperature sensors
CREATE TABLE sensors (
    id          SERIAL PRIMARY KEY,
    name        TEXT    NOT NULL,
    location    TEXT    NOT NULL,
    temperature DOUBLE PRECISION NOT NULL DEFAULT 0.0
);

-- Seed data
INSERT INTO sensors (name, location, temperature) VALUES
    ('sensor-1', 'Building A',  72.5),
    ('sensor-2', 'Building A',  78.3),
    ('sensor-3', 'Building B',  65.1),
    ('sensor-4', 'Building C',  81.7);

-- Replication slot + publication for Drasi CDC
SELECT pg_create_logical_replication_slot('drasi_slot', 'pgoutput');
CREATE PUBLICATION drasi_pub FOR TABLE sensors;
