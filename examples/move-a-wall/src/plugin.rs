use crate::{source::SceneSource, transformer::Geometry, wire};
use anyhow::Result;
use drasi_computation_plugin_sdk::{
    Capabilities, Component, ConfigSchema, ControlSender, CreateRequest, CreatedComponent, Factory,
    FactoryMetadata, PluginDefinition,
};
use drasi_lib::computation::v1::*;
use std::sync::Arc;

struct NativeFactory {
    source: bool,
}
impl Factory for NativeFactory {
    fn metadata(&self) -> FactoryMetadata {
        FactoryMetadata {
            implementation: ImplementationIdentity::try_new(
                if self.source {
                    "move-a-wall/scene"
                } else {
                    "move-a-wall/geometry"
                },
                "1",
            )
            .expect("constant identity"),
            role: if self.source {
                ComponentRole::Source
            } else {
                ComponentRole::Transformer
            },
            configuration_version: 1,
            configuration: ConfigSchema::default(),
            ports: if self.source {
                vec![wire::port("out", PortDirection::Output, false).expect("source port")]
            } else {
                vec![
                    wire::port("in", PortDirection::Input, true).expect("input port"),
                    wire::port("out", PortDirection::Output, false).expect("output port"),
                ]
            },
            completion: None,
            capabilities: Capabilities::default(),
        }
    }
    fn create(&self, request: &CreateRequest, _: ControlSender) -> Result<CreatedComponent> {
        self.metadata()
            .configuration
            .validate(&request.configuration)?;
        Ok(if self.source {
            Component::Source(Box::new(SceneSource::hosted(
                self.metadata().descriptor(request.id.clone())?,
            )?))
            .into()
        } else {
            Component::Transformer(Box::new(Geometry::new(request.id.clone())?)).into()
        })
    }
}
pub fn plugin() -> Result<PluginDefinition> {
    PluginDefinition::new(
        "move-a-wall",
        env!("CARGO_PKG_VERSION"),
        vec![
            Arc::new(NativeFactory { source: true }),
            Arc::new(NativeFactory { source: false }),
        ],
        vec![GraphChangeCodec::schema(), QueryChangeCodec::schema()],
    )
}
