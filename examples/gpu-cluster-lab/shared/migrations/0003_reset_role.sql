DO $$
BEGIN
    IF NOT EXISTS (SELECT FROM pg_roles WHERE rolname = 'gpu_reset') THEN
        CREATE ROLE gpu_reset NOLOGIN;
    END IF;
END $$;
GRANT USAGE ON SCHEMA public TO gpu_reset;
GRANT SELECT, INSERT, UPDATE, DELETE ON regional_clusters, placement_policies, data_profiles,
    gpu_inventory, gpu_telemetry, workload_requirements TO gpu_reset;
GRANT SELECT, INSERT, UPDATE ON gpu_placements TO gpu_reset;
GRANT DELETE ON command_receipts TO gpu_reset;
