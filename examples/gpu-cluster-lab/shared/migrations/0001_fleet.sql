CREATE TABLE regional_clusters (
    cluster_id text PRIMARY KEY CHECK (length(trim(cluster_id)) BETWEEN 1 AND 256),
    name text NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 256),
    region text NOT NULL CHECK (region IN ('westeurope','northeurope','eastus')),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE placement_policies (
    policy_id text PRIMARY KEY CHECK (length(trim(policy_id)) BETWEEN 1 AND 256),
    name text NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 256),
    customer_id text NOT NULL CHECK (length(trim(customer_id)) BETWEEN 1 AND 256),
    allowed_regions jsonb NOT NULL CHECK (jsonb_typeof(allowed_regions) = 'array'),
    allowed_purposes jsonb NOT NULL CHECK (jsonb_typeof(allowed_purposes) = 'array'),
    allowed_classifications jsonb NOT NULL CHECK (jsonb_typeof(allowed_classifications) = 'array'),
    authority_ref text NOT NULL CHECK (length(trim(authority_ref)) BETWEEN 1 AND 256),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE data_profiles (
    data_profile_id text PRIMARY KEY CHECK (length(trim(data_profile_id)) BETWEEN 1 AND 256),
    customer_id text NOT NULL CHECK (length(trim(customer_id)) BETWEEN 1 AND 256),
    classification text NOT NULL CHECK (classification IN ('synthetic','restricted')),
    policy_id text NOT NULL REFERENCES placement_policies ON DELETE RESTRICT,
    authority_ref text NOT NULL CHECK (length(trim(authority_ref)) BETWEEN 1 AND 256),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE gpu_inventory (
    gpu_id uuid PRIMARY KEY,
    name text NOT NULL UNIQUE CHECK (length(trim(name)) BETWEEN 1 AND 256),
    cluster_id text NOT NULL REFERENCES regional_clusters ON DELETE RESTRICT,
    host_id text NOT NULL CHECK (length(trim(host_id)) BETWEEN 1 AND 256),
    gpu_index smallint NOT NULL CHECK (gpu_index IN (0,1)),
    model text NOT NULL CHECK (model = 'NVIDIA H100 NVL'),
    vm_size text NOT NULL CHECK (vm_size = 'Standard_NC80adis_H100_v5'),
    nominal_vram_gb integer NOT NULL CHECK (nominal_vram_gb = 94),
    memory_mib integer NOT NULL CHECK (memory_mib = 81920),
    compute_units integer NOT NULL CHECK (compute_units = 100),
    failure_domain text NOT NULL CHECK (failure_domain = host_id),
    scheduling_enabled boolean NOT NULL DEFAULT true,
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now(),
    UNIQUE (host_id, gpu_index)
);
CREATE TABLE gpu_telemetry (
    gpu_id uuid PRIMARY KEY REFERENCES gpu_inventory ON DELETE CASCADE,
    powered_on boolean NOT NULL DEFAULT true,
    reporting_enabled boolean NOT NULL DEFAULT true,
    interval_ms integer NOT NULL DEFAULT 1000 CHECK (interval_ms BETWEEN 250 AND 2000),
    background_compute_units integer NOT NULL DEFAULT 10 CHECK (background_compute_units BETWEEN 0 AND 200),
    background_memory_mib integer NOT NULL DEFAULT 0 CHECK (background_memory_mib BETWEEN 0 AND 81920),
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE workload_requirements (
    workload_id uuid PRIMARY KEY,
    name text NOT NULL UNIQUE CHECK (length(trim(name)) BETWEEN 1 AND 256),
    model_ref text NOT NULL CHECK (length(trim(model_ref)) BETWEEN 1 AND 256),
    profile_id text NOT NULL CHECK (profile_id IN ('assistant-v1','chat-v1','embeddings-v1','reranker-v1','custom')),
    data_profile_id text NOT NULL REFERENCES data_profiles ON DELETE RESTRICT,
    purpose text NOT NULL CHECK (purpose IN ('demo','customer-support')),
    replicas integer NOT NULL CHECK (replicas BETWEEN 0 AND 32),
    memory_mib_per_replica integer NOT NULL CHECK (memory_mib_per_replica BETWEEN 1 AND 81920),
    compute_units_per_replica integer NOT NULL CHECK (compute_units_per_replica BETWEEN 1 AND 200),
    allowed_gpu_models jsonb NOT NULL CHECK (
        jsonb_typeof(allowed_gpu_models) = 'array' AND jsonb_array_length(allowed_gpu_models) > 0
    ),
    spread_across_domains boolean NOT NULL DEFAULT true,
    revision bigint NOT NULL DEFAULT 1 CHECK (revision > 0),
    updated_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE gpu_placements (
    fleet_id text PRIMARY KEY CHECK (fleet_id = 'demo'),
    plan_version bigint NOT NULL CHECK (plan_version >= 0),
    decision_id uuid NOT NULL,
    config_fingerprint text NOT NULL,
    policy_signature text NOT NULL,
    policy_bundle_hash text NOT NULL,
    assignments jsonb NOT NULL CHECK (jsonb_typeof(assignments) = 'array'),
    decision_details jsonb NOT NULL CHECK (
        jsonb_typeof(decision_details) = 'object' AND octet_length(decision_details::text) <= 131072
    ),
    committed_at timestamptz NOT NULL DEFAULT now()
);
CREATE TABLE command_receipts (
    operation_kind text NOT NULL,
    request_key text NOT NULL,
    payload_hash text NOT NULL,
    response jsonb NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (operation_kind, request_key)
);

CREATE FUNCTION gpu_fleet_lock() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    PERFORM pg_advisory_xact_lock(778807123);
    RETURN NULL;
END $$;

CREATE FUNCTION gpu_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        NEW.revision := 1;
        NEW.updated_at := clock_timestamp();
    ELSIF (to_jsonb(NEW) - 'revision' - 'updated_at') =
          (to_jsonb(OLD) - 'revision' - 'updated_at') THEN
        NEW.revision := OLD.revision;
        NEW.updated_at := OLD.updated_at;
    ELSE
        NEW.revision := OLD.revision + 1;
        NEW.updated_at := clock_timestamp();
    END IF;
    RETURN NEW;
END $$;

CREATE FUNCTION gpu_immutable() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF TG_TABLE_NAME = 'regional_clusters' THEN
        IF NEW.cluster_id <> OLD.cluster_id OR NEW.region <> OLD.region THEN
            RAISE EXCEPTION 'cluster identity and region are immutable' USING ERRCODE = '23514';
        END IF;
    ELSIF TG_TABLE_NAME = 'gpu_inventory' THEN
        IF (to_jsonb(NEW) - 'name' - 'scheduling_enabled' - 'revision' - 'updated_at') <>
           (to_jsonb(OLD) - 'name' - 'scheduling_enabled' - 'revision' - 'updated_at') THEN
            RAISE EXCEPTION 'GPU hardware and membership are immutable' USING ERRCODE = '23514';
        END IF;
    END IF;
    RETURN NEW;
END $$;
CREATE TRIGGER immutable BEFORE UPDATE ON regional_clusters FOR EACH ROW EXECUTE FUNCTION gpu_immutable();
CREATE TRIGGER immutable BEFORE UPDATE ON gpu_inventory FOR EACH ROW EXECUTE FUNCTION gpu_immutable();

CREATE FUNCTION gpu_valid_set(value jsonb, allowed text[]) RETURNS boolean
LANGUAGE sql IMMUTABLE STRICT AS $$
    SELECT jsonb_typeof(value) = 'array'
       AND NOT EXISTS (SELECT FROM jsonb_array_elements(value) v WHERE jsonb_typeof(v) <> 'string')
       AND NOT EXISTS (SELECT FROM jsonb_array_elements_text(value) v WHERE NOT v = ANY(allowed))
       AND (SELECT count(*) = count(DISTINCT v) FROM jsonb_array_elements_text(value) v)
$$;

CREATE FUNCTION gpu_validate_fleet() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF (SELECT count(*) > 8 FROM regional_clusters)
       OR (SELECT count(*) > 16 OR count(DISTINCT host_id) > 8 FROM gpu_inventory)
       OR (SELECT count(*) > 32 OR coalesce(sum(replicas),0) > 32 FROM workload_requirements)
       OR (SELECT count(*) > 32 FROM placement_policies)
       OR (SELECT count(*) > 32 FROM data_profiles) THEN
        RAISE EXCEPTION 'demo fleet bounds exceeded' USING ERRCODE = '23514';
    END IF;
    IF EXISTS (SELECT FROM gpu_inventory GROUP BY host_id HAVING count(DISTINCT cluster_id) > 1) THEN
        RAISE EXCEPTION 'worker cannot span clusters' USING ERRCODE = '23514';
    END IF;
    IF EXISTS (SELECT FROM data_profiles d JOIN placement_policies p USING (policy_id)
               WHERE d.customer_id <> p.customer_id) THEN
        RAISE EXCEPTION 'data/policy customer mismatch' USING ERRCODE = '23514';
    END IF;
    IF EXISTS (
        SELECT FROM placement_policies p
        WHERE NOT gpu_valid_set(allowed_regions, ARRAY['westeurope','northeurope','eastus','*'])
           OR NOT gpu_valid_set(allowed_purposes, ARRAY['demo','customer-support'])
           OR NOT gpu_valid_set(allowed_classifications, ARRAY['synthetic','restricted'])
           OR (allowed_regions ? '*' AND NOT (
                policy_id = 'demo-permissive' AND customer_id = 'demo'
                AND allowed_regions = '["*"]' AND allowed_purposes = '["demo"]'
                AND allowed_classifications = '["synthetic"]'))
    ) THEN
        RAISE EXCEPTION 'invalid policy parameters' USING ERRCODE = '23514';
    END IF;
    IF EXISTS (
        SELECT FROM workload_requirements
        WHERE NOT gpu_valid_set(allowed_gpu_models, ARRAY['NVIDIA H100 NVL','*'])
           OR (profile_id = 'assistant-v1' AND
               (model_ref <> 'Qwen/Qwen2.5-32B-Instruct' OR memory_mib_per_replica <> 77824 OR compute_units_per_replica <> 55))
           OR (profile_id = 'chat-v1' AND
               (model_ref <> 'meta-llama/Llama-3.1-8B-Instruct' OR memory_mib_per_replica <> 24576 OR compute_units_per_replica <> 30))
           OR (profile_id = 'embeddings-v1' AND
               (model_ref <> 'BAAI/bge-m3' OR memory_mib_per_replica <> 4096 OR compute_units_per_replica <> 20))
           OR (profile_id = 'reranker-v1' AND
               (model_ref <> 'BAAI/bge-reranker-v2-m3' OR memory_mib_per_replica <> 4096 OR compute_units_per_replica <> 25))
    ) THEN
        RAISE EXCEPTION 'invalid serving profile or GPU model set' USING ERRCODE = '23514';
    END IF;
    RETURN NULL;
END $$;

DO $$
DECLARE t text;
BEGIN
    FOREACH t IN ARRAY ARRAY['regional_clusters','placement_policies','data_profiles',
        'gpu_inventory','gpu_telemetry','workload_requirements'] LOOP
        EXECUTE format('CREATE TRIGGER fleet_lock BEFORE INSERT OR UPDATE OR DELETE ON %I FOR EACH STATEMENT EXECUTE FUNCTION gpu_fleet_lock()',t);
        EXECUTE format('CREATE TRIGGER revision BEFORE INSERT OR UPDATE ON %I FOR EACH ROW EXECUTE FUNCTION gpu_revision()',t);
        EXECUTE format('CREATE TRIGGER fleet_bounds AFTER INSERT OR UPDATE OR DELETE ON %I FOR EACH STATEMENT EXECUTE FUNCTION gpu_validate_fleet()',t);
        EXECUTE format('ALTER TABLE %I REPLICA IDENTITY FULL',t);
    END LOOP;
END $$;
CREATE TRIGGER fleet_lock BEFORE INSERT OR UPDATE OR DELETE ON gpu_placements
    FOR EACH STATEMENT EXECUTE FUNCTION gpu_fleet_lock();
ALTER TABLE gpu_placements REPLICA IDENTITY FULL;

CREATE PUBLICATION gpu_demo_publication FOR TABLE regional_clusters, placement_policies,
    data_profiles, gpu_inventory, gpu_telemetry, workload_requirements, gpu_placements;
