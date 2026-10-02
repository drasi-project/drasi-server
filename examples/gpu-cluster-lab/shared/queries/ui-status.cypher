MATCH (r:DemoReadiness)
OPTIONAL MATCH (f:FleetConfiguration)
OPTIONAL MATCH (p:PolicyAssessment)
RETURN r.fleet_id AS fleet_id, r.observation_epoch AS observation_epoch,
       r.scheduling_signature AS scheduling_signature, r.policy_signature AS policy_signature,
       r.scenario_ready AS scenario_ready, r.inputs_ready AS inputs_ready, r.scenario AS scenario,
       r.state AS state, r.detail AS detail, r.components AS components,
       CASE WHEN f.epoch = r.observation_epoch THEN f.configuration.policies ELSE null END AS policy_rules,
       CASE WHEN f.epoch = r.observation_epoch THEN f.configuration.data_profiles ELSE null END AS data_profiles,
       CASE WHEN f.epoch = r.observation_epoch AND p.epoch = f.epoch
         AND p.config_fingerprint = f.config_fingerprint
         AND p.assessment.policy_signature = r.policy_signature AND r.inputs_ready = true
         THEN true ELSE false END AS policy_rules_current
