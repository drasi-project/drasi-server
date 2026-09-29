GRANT USAGE ON SCHEMA public TO gpu_config, gpu_plan, gpu_reader;
GRANT SELECT, INSERT, UPDATE, DELETE ON regional_clusters, placement_policies, data_profiles,
    gpu_inventory, gpu_telemetry, workload_requirements TO gpu_config;
GRANT SELECT ON regional_clusters, placement_policies, data_profiles, gpu_inventory,
    gpu_telemetry, workload_requirements TO gpu_plan;
GRANT SELECT, INSERT, UPDATE ON gpu_placements TO gpu_plan;
GRANT SELECT, INSERT ON command_receipts TO gpu_config, gpu_plan;
GRANT SELECT ON regional_clusters, placement_policies, data_profiles, gpu_inventory,
    gpu_telemetry, workload_requirements, gpu_placements TO gpu_reader;
