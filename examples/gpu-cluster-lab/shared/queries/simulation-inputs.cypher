MATCH (f:FleetConfiguration)
RETURN {epoch:f.epoch, configuration:f.configuration, settings:f.settings, plan:f.plan} AS snapshot
