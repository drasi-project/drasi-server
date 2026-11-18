use crate::{source::SceneSource, wire};
use anyhow::Result;
use drasi_computation_contracts::*;
use drasi_computation_plugin_sdk::{
    Capabilities, Component, ConfigSchema, ControlSender, CreateRequest, CreatedComponent, Factory,
    FactoryMetadata, PluginDefinition,
};
use std::sync::Arc;

/// The demo's only native component: the volatile scene source. Geometry is
/// computed by the standard `drasi.standard/geometry` transformer.
struct SceneFactory;
impl Factory for SceneFactory {
    fn describe(
        &self,
        request: &drasi_computation_plugin_sdk::wire::DescribeRequest,
    ) -> Result<ComponentDescription> {
        Ok(request.source_description(
            OutputDescription::VOLATILE,
            drasi_computation_plugin_sdk::ProgressPersistence::Undeclared,
        ))
    }
    fn metadata(&self) -> FactoryMetadata {
        FactoryMetadata {
            implementation: ImplementationIdentity::try_new("move-a-wall/scene", "1")
                .expect("constant identity"),
            role: ComponentRole::Source,
            configuration_version: 1,
            configuration: ConfigSchema::default(),
            ports: vec![wire::port("out", PortDirection::Output).expect("source port")],
            completion: None,
            capabilities: Capabilities::default(),
        }
    }
    fn create(&self, request: &CreateRequest, _: ControlSender) -> Result<CreatedComponent> {
        self.metadata()
            .configuration
            .validate(&request.configuration)?;
        Ok(Component::Source(Box::new(SceneSource::hosted(
            self.metadata().descriptor(request.id.clone())?,
            request.scope.as_ref(),
        )?))
        .into())
    }
}
pub fn plugin() -> Result<PluginDefinition> {
    PluginDefinition::new(
        "move-a-wall",
        env!("CARGO_PKG_VERSION"),
        vec![Arc::new(SceneFactory)],
        vec![GraphChangeCodec::schema()],
    )
}
