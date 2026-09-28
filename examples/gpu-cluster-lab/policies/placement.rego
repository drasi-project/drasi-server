package gpu.placement
import rego.v1

region_allowed if input.cluster.region in input.policy.allowed_regions
region_allowed if input.policy.allowed_regions == ["*"]

reasons contains "region-not-permitted" if not region_allowed
reasons contains "purpose-not-permitted" if not input.workload.purpose in input.policy.allowed_purposes
reasons contains "classification-not-permitted" if not input.data_profile.classification in input.policy.allowed_classifications
reasons contains "customer-mismatch" if input.data_profile.customer_id != input.policy.customer_id

default allow := false
allow if count(reasons) == 0

decision := {"allow": allow, "reasons": sort(reasons)}
