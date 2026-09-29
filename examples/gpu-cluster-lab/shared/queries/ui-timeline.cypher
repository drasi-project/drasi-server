MATCH (e:DemoEvent)
RETURN e.event_id AS event_id, e.observation_epoch AS observation_epoch,
       e.event_sequence AS event_sequence, e.time_ms AS time_ms,
       e.component_id AS component_id, e.kind AS kind, e.message AS message,
       e.decision_id AS decision_id, e.plan_version AS plan_version
