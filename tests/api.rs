//! Main entry point for API test suite

#[path = "api/contract_test.rs"]
mod contract_test;

#[path = "api/integration_test.rs"]
mod integration_test;

#[path = "api/state_consistency_test.rs"]
mod state_consistency_test;

// OpenAPI validation tests removed as external types from drasi-server-core
// don't implement ToSchema trait required for documentation generation
