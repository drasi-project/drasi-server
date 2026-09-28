CREATE TABLE demo_reset_state (
    fleet_id text PRIMARY KEY CHECK (fleet_id = 'demo'),
    pending boolean NOT NULL
);
INSERT INTO demo_reset_state (fleet_id, pending) VALUES ('demo', false);
GRANT SELECT, UPDATE ON demo_reset_state TO gpu_reset;
