MATCH (r:DemoReadiness)
RETURN r.fleet_id AS fleet_id, r.observation_epoch AS observation_epoch,
       r.scheduling_signature AS scheduling_signature, r.policy_signature AS policy_signature,
       r.scenario_ready AS scenario_ready, r.inputs_ready AS inputs_ready, r.scenario AS scenario,
       r.state AS state, r.detail AS detail, r.components AS components
