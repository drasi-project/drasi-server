package gpu.eligibility
import rego.v1

# Wraps gpu.placement with the lab's context validation. Invalid or missing
# context is never authorized: it decides "unknown".

valid_context if {
    is_object(input.data_profile)
    is_object(input.policy)
    input.data_profile.policy_id == input.policy.policy_id
    input.data_profile.classification in {"synthetic", "restricted"}
    to_number(input.data_profile.revision) > 0
    to_number(input.policy.revision) > 0
    input.data_profile.authority_ref != ""
    input.policy.authority_ref != ""
    input.workload.purpose in {"demo", "customer-support"}
}

default result := {"authorization": "unknown", "reasons": ["invalid-policy-context"]}

result := {"authorization": "allow", "reasons": []} if {
    valid_context
    data.gpu.placement.decision.allow
}

result := {"authorization": "deny", "reasons": data.gpu.placement.decision.reasons} if {
    valid_context
    not data.gpu.placement.decision.allow
}
