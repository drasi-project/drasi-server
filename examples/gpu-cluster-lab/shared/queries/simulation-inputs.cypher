MATCH (f:FleetConfiguration)
OPTIONAL MATCH (f)-[:POLICY_CONFIG]->(p:PolicyAssessment)
RETURN {epoch:f.epoch, configuration:f.configuration, settings:f.settings, plan:f.plan} AS snapshot,
       p.assessment AS policy, p.epoch AS policy_epoch
