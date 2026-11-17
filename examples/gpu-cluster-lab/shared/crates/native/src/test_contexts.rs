//! Lifecycle contexts for driving components directly in tests and tools,
//! outside a graph.

use drasi_computation_plugin_sdk::lifecycle::{CancellationToken, OperationScope, StopMode};

fn scope() -> OperationScope {
    OperationScope::new("test", "component", 1, None, CancellationToken::new())
}

/// Contexts for DrasiLib components.
#[cfg(any(test, feature = "runtime", feature = "query-isolation"))]
pub mod lib {
    use drasi_lib::computation::v1::{
        DisposeContext, DisposeReason, InitializeContext, StartContext, StateIntent, StopContext,
    };

    pub fn initialize() -> InitializeContext {
        InitializeContext::new(super::scope())
    }
    pub fn start() -> StartContext {
        StartContext::new(super::scope())
    }
    pub fn stop() -> StopContext {
        StopContext::new(super::scope(), super::StopMode::Drain)
    }
    pub fn dispose() -> DisposeContext {
        DisposeContext::new(
            super::scope(),
            DisposeReason::Shutdown,
            StateIntent::Preserve,
            1,
        )
        .expect("shutdown preserves state")
    }
}

/// Contexts for native SDK components.
pub mod sdk {
    use drasi_computation_plugin_sdk::lifecycle::{
        DisposeContext, DisposeReason, InitializeContext, StartContext, StateIntent, StopContext,
    };

    pub fn initialize() -> InitializeContext {
        InitializeContext::new(super::scope())
    }
    pub fn start() -> StartContext {
        StartContext::new(super::scope(), None)
    }
    pub fn stop() -> StopContext {
        StopContext::new(super::scope(), super::StopMode::Drain)
    }
    pub fn dispose() -> DisposeContext {
        DisposeContext::new(
            super::scope(),
            DisposeReason::Shutdown,
            StateIntent::Preserve,
            1,
        )
        .expect("shutdown preserves state")
    }
}

/// The deterministic component ID the lab assigns to a readable name.
pub fn component_id(
    name: impl AsRef<str>,
) -> anyhow::Result<drasi_computation_contracts::ComponentId> {
    Ok(drasi_computation_contracts::ComponentId::try_new(
        gpu_contracts::component_id(name.as_ref()),
    )?)
}
