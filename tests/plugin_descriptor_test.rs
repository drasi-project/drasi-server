//! Per-plugin descriptor tests.
//!
//! Verifies that every built-in plugin returns correct kind, config_version,
//! and valid config_schema_json from its descriptor.

use drasi_server::builtin_plugins::register_builtin_plugins;
use drasi_server::plugin_registry::PluginRegistry;
use drasi_server::register_core_plugins;

fn populated_registry() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    register_core_plugins(&mut registry);
    register_builtin_plugins(&mut registry);
    registry
}

// ==========================================================================
// Source descriptor tests
// ==========================================================================

const EXPECTED_SOURCE_KINDS: &[&str] = &["grpc", "http", "mock", "mssql", "postgres"];

#[test]
fn test_all_source_kinds_registered() {
    let registry = populated_registry();
    let kinds = registry.source_kinds();
    assert_eq!(
        kinds, EXPECTED_SOURCE_KINDS,
        "Source kinds mismatch: got {:?}",
        kinds
    );
}

#[test]
fn test_source_descriptors_have_valid_kind() {
    let registry = populated_registry();
    for kind in EXPECTED_SOURCE_KINDS {
        let descriptor = registry
            .get_source(kind)
            .unwrap_or_else(|| panic!("Source '{kind}' should be registered"));
        assert_eq!(descriptor.kind(), *kind);
    }
}

#[test]
fn test_source_descriptors_have_semver_config_version() {
    let registry = populated_registry();
    for kind in EXPECTED_SOURCE_KINDS {
        let descriptor = registry.get_source(kind).unwrap();
        let version = descriptor.config_version();
        assert!(
            semver::Version::parse(version).is_ok(),
            "Source '{kind}' config_version '{version}' is not valid semver"
        );
    }
}

#[test]
fn test_source_descriptors_have_valid_json_schema() {
    let registry = populated_registry();
    for kind in EXPECTED_SOURCE_KINDS {
        let descriptor = registry.get_source(kind).unwrap();
        let schema_json = descriptor.config_schema_json();
        assert!(
            !schema_json.is_empty(),
            "Source '{kind}' config_schema_json is empty"
        );
        let parsed: serde_json::Value = serde_json::from_str(&schema_json).unwrap_or_else(|e| {
            panic!("Source '{kind}' config_schema_json is not valid JSON: {e}")
        });
        assert!(
            parsed.is_object(),
            "Source '{kind}' config_schema_json should be a JSON object"
        );
    }
}

// ==========================================================================
// Reaction descriptor tests
// ==========================================================================

const EXPECTED_REACTION_KINDS: &[&str] = &[
    "application",
    "grpc",
    "grpc-adaptive",
    "http",
    "http-adaptive",
    "log",
    "profiler",
    "sse",
    "storedproc-mssql",
    "storedproc-mysql",
    "storedproc-postgres",
];

#[test]
fn test_all_reaction_kinds_registered() {
    let registry = populated_registry();
    let kinds = registry.reaction_kinds();
    assert_eq!(
        kinds, EXPECTED_REACTION_KINDS,
        "Reaction kinds mismatch: got {:?}",
        kinds
    );
}

#[test]
fn test_reaction_descriptors_have_valid_kind() {
    let registry = populated_registry();
    for kind in EXPECTED_REACTION_KINDS {
        let descriptor = registry
            .get_reaction(kind)
            .unwrap_or_else(|| panic!("Reaction '{kind}' should be registered"));
        assert_eq!(descriptor.kind(), *kind);
    }
}

#[test]
fn test_reaction_descriptors_have_semver_config_version() {
    let registry = populated_registry();
    for kind in EXPECTED_REACTION_KINDS {
        let descriptor = registry.get_reaction(kind).unwrap();
        let version = descriptor.config_version();
        assert!(
            semver::Version::parse(version).is_ok(),
            "Reaction '{kind}' config_version '{version}' is not valid semver"
        );
    }
}

#[test]
fn test_reaction_descriptors_have_valid_json_schema() {
    let registry = populated_registry();
    for kind in EXPECTED_REACTION_KINDS {
        let descriptor = registry.get_reaction(kind).unwrap();
        let schema_json = descriptor.config_schema_json();
        assert!(
            !schema_json.is_empty(),
            "Reaction '{kind}' config_schema_json is empty"
        );
        let _parsed: serde_json::Value = serde_json::from_str(&schema_json).unwrap_or_else(|e| {
            panic!("Reaction '{kind}' config_schema_json is not valid JSON: {e}")
        });
    }
}

// ==========================================================================
// Bootstrap descriptor tests
// ==========================================================================

const EXPECTED_BOOTSTRAP_KINDS: &[&str] =
    &["application", "mssql", "noop", "postgres", "scriptfile"];

#[test]
fn test_all_bootstrap_kinds_registered() {
    let registry = populated_registry();
    let kinds = registry.bootstrapper_kinds();
    assert_eq!(
        kinds, EXPECTED_BOOTSTRAP_KINDS,
        "Bootstrapper kinds mismatch: got {:?}",
        kinds
    );
}

#[test]
fn test_bootstrap_descriptors_have_valid_kind() {
    let registry = populated_registry();
    for kind in EXPECTED_BOOTSTRAP_KINDS {
        let descriptor = registry
            .get_bootstrapper(kind)
            .unwrap_or_else(|| panic!("Bootstrapper '{kind}' should be registered"));
        assert_eq!(descriptor.kind(), *kind);
    }
}

#[test]
fn test_bootstrap_descriptors_have_semver_config_version() {
    let registry = populated_registry();
    for kind in EXPECTED_BOOTSTRAP_KINDS {
        let descriptor = registry.get_bootstrapper(kind).unwrap();
        let version = descriptor.config_version();
        assert!(
            semver::Version::parse(version).is_ok(),
            "Bootstrapper '{kind}' config_version '{version}' is not valid semver"
        );
    }
}

#[test]
fn test_bootstrap_descriptors_have_valid_json_schema() {
    let registry = populated_registry();
    for kind in EXPECTED_BOOTSTRAP_KINDS {
        let descriptor = registry.get_bootstrapper(kind).unwrap();
        let schema_json = descriptor.config_schema_json();
        assert!(
            !schema_json.is_empty(),
            "Bootstrapper '{kind}' config_schema_json is empty"
        );
        let parsed: serde_json::Value = serde_json::from_str(&schema_json).unwrap_or_else(|e| {
            panic!("Bootstrapper '{kind}' config_schema_json is not valid JSON: {e}")
        });
        assert!(
            parsed.is_object(),
            "Bootstrapper '{kind}' config_schema_json should be a JSON object"
        );
    }
}

// ==========================================================================
// Cross-cutting tests
// ==========================================================================

#[test]
fn test_total_descriptor_count() {
    let registry = populated_registry();
    let expected = EXPECTED_SOURCE_KINDS.len()
        + EXPECTED_REACTION_KINDS.len()
        + EXPECTED_BOOTSTRAP_KINDS.len();
    assert_eq!(
        registry.descriptor_count(),
        expected,
        "Expected {expected} total descriptors, got {}",
        registry.descriptor_count()
    );
}

#[test]
fn test_all_config_versions_are_1_0_0() {
    let registry = populated_registry();

    for kind in EXPECTED_SOURCE_KINDS {
        let d = registry.get_source(kind).unwrap();
        assert_eq!(
            d.config_version(),
            "1.0.0",
            "Source '{kind}' should be version 1.0.0"
        );
    }
    for kind in EXPECTED_REACTION_KINDS {
        let d = registry.get_reaction(kind).unwrap();
        assert_eq!(
            d.config_version(),
            "1.0.0",
            "Reaction '{kind}' should be version 1.0.0"
        );
    }
    for kind in EXPECTED_BOOTSTRAP_KINDS {
        let d = registry.get_bootstrapper(kind).unwrap();
        assert_eq!(
            d.config_version(),
            "1.0.0",
            "Bootstrapper '{kind}' should be version 1.0.0"
        );
    }
}

#[test]
fn test_plugin_infos_match_descriptors() {
    let registry = populated_registry();

    let source_infos = registry.source_plugin_infos();
    assert_eq!(source_infos.len(), EXPECTED_SOURCE_KINDS.len());
    for (info, expected) in source_infos.iter().zip(EXPECTED_SOURCE_KINDS.iter()) {
        assert_eq!(info.kind, *expected);
    }

    let reaction_infos = registry.reaction_plugin_infos();
    assert_eq!(reaction_infos.len(), EXPECTED_REACTION_KINDS.len());
    for (info, expected) in reaction_infos.iter().zip(EXPECTED_REACTION_KINDS.iter()) {
        assert_eq!(info.kind, *expected);
    }

    let bootstrap_infos = registry.bootstrapper_plugin_infos();
    assert_eq!(bootstrap_infos.len(), EXPECTED_BOOTSTRAP_KINDS.len());
    for (info, expected) in bootstrap_infos.iter().zip(EXPECTED_BOOTSTRAP_KINDS.iter()) {
        assert_eq!(info.kind, *expected);
    }
}
