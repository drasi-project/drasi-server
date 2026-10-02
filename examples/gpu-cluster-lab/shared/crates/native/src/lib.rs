pub mod bootstrap;
mod evidence;
pub mod inputs;
pub mod lifecycle;
pub mod postgres;
mod processor;
pub mod projections;
mod reaction;
mod status;
pub mod wire;
pub use status::{RuntimeComponent, RuntimeObservation};

use anyhow::Result;
use drasi_computation_plugin_sdk::{
    Capabilities, Component, ConfigField, ConfigSchema, ConfigType, ControlSender, CreateRequest,
    CreatedComponent, Factory, FactoryMetadata, PluginDefinition,
};
use drasi_lib::computation::v1::{
    ComponentRole, GraphChangeCodec, ImplementationIdentity, PipeRequirements, PortDescriptor,
    PortDirection, PortId, QueryChangeCodec,
};
use std::{
    collections::BTreeMap,
    sync::{atomic::AtomicBool, Arc},
};
use tokio::sync::Semaphore;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Simulator,
    Policy,
    Placement,
    Resilience,
}
impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Simulator => "gpu.lab/telemetry-simulator",
            Self::Policy => "gpu.lab/regorus-policy",
            Self::Placement => "gpu.lab/placement-solver",
            Self::Resilience => "gpu.lab/resilience-assessor",
        }
    }
}
pub struct Workers {
    solver: Arc<Semaphore>,
    policy: Arc<Semaphore>,
    placement_pending: AtomicBool,
    hub: Arc<status::Hub>,
}
impl Default for Workers {
    fn default() -> Self {
        Self {
            solver: Arc::new(Semaphore::new(1)),
            policy: Arc::new(Semaphore::new(1)),
            placement_pending: AtomicBool::new(false),
            hub: Arc::new(status::Hub::default()),
        }
    }
}
impl Workers {
    pub fn observe_runtime(&self, observation: RuntimeObservation) -> Result<()> {
        self.hub.observe_runtime(observation)
    }
}
pub struct NativeFactory {
    pub kind: Kind,
    pub workers: Arc<Workers>,
}
impl Factory for NativeFactory {
    fn metadata(&self) -> FactoryMetadata {
        FactoryMetadata {
            implementation: ImplementationIdentity::try_new(self.kind.name(), "1")
                .expect("constant implementation"),
            role: ComponentRole::Transformer,
            configuration_version: 1,
            configuration: ConfigSchema {
                fields: BTreeMap::from([(
                    "stream".into(),
                    ConfigField {
                        value_type: ConfigType::String,
                        required: true,
                        secret: false,
                    },
                )]),
                allow_additional: false,
            },
            ports: vec![
                PortDescriptor::new(
                    PortId::try_new("in").expect("constant port"),
                    PortDirection::Input,
                    QueryChangeCodec::schema().descriptor().clone(),
                    PipeRequirements::default(),
                ),
                PortDescriptor::new(
                    PortId::try_new("out").expect("constant port"),
                    PortDirection::Output,
                    GraphChangeCodec::schema().descriptor().clone(),
                    PipeRequirements::default(),
                ),
            ],
            completion: None,
            capabilities: Capabilities {
                control: true,
                wakeups: true,
                ..Capabilities::default()
            },
        }
    }
    fn create(&self, request: &CreateRequest, _control: ControlSender) -> Result<CreatedComponent> {
        let processor = processor::Processor::new(
            self.kind,
            self.metadata().descriptor(request.id.clone())?,
            request.configuration.clone(),
            self.workers.clone(),
        )?;
        let control_handler = Some(Arc::new(lifecycle::Handler {
            signals: Some(processor.signals.clone()),
            hub: self.workers.hub.clone(),
        })
            as Arc<dyn drasi_computation_plugin_sdk::NativeControlHandler>);
        Ok(CreatedComponent {
            component: Component::Transformer(Box::new(processor)),
            control_handler,
        })
    }
}
pub fn plugin() -> Result<PluginDefinition> {
    let workers = Arc::new(Workers::default());
    let mut factories: Vec<Arc<dyn Factory>> = [
        Kind::Simulator,
        Kind::Policy,
        Kind::Placement,
        Kind::Resilience,
    ]
    .into_iter()
    .map(|kind| {
        Arc::new(NativeFactory {
            kind,
            workers: workers.clone(),
        }) as Arc<dyn Factory>
    })
    .collect();
    factories.push(Arc::new(AuxiliaryFactory {
        reaction: false,
        hub: workers.hub.clone(),
    }));
    factories.push(Arc::new(AuxiliaryFactory {
        reaction: true,
        hub: workers.hub.clone(),
    }));
    PluginDefinition::new(
        "gpu-cluster-lab",
        env!("CARGO_PKG_VERSION"),
        factories,
        vec![GraphChangeCodec::schema(), QueryChangeCodec::schema()],
    )
}

pub struct AuxiliaryFactory {
    reaction: bool,
    hub: Arc<status::Hub>,
}
impl Factory for AuxiliaryFactory {
    fn metadata(&self) -> FactoryMetadata {
        let mut fields = BTreeMap::new();
        for (name, secret) in if self.reaction {
            vec![("endpoint", false), ("token", true)]
        } else {
            vec![("stream", false)]
        } {
            fields.insert(
                name.into(),
                ConfigField {
                    value_type: ConfigType::String,
                    required: true,
                    secret,
                },
            );
        }
        FactoryMetadata {
            implementation: ImplementationIdentity::try_new(
                if self.reaction {
                    "gpu.lab/plan-writer"
                } else {
                    "gpu.lab/runtime-status"
                },
                "1",
            )
            .expect("constant identity"),
            role: if self.reaction {
                ComponentRole::Sink
            } else {
                ComponentRole::Source
            },
            configuration_version: 1,
            configuration: ConfigSchema {
                fields,
                allow_additional: false,
            },
            ports: vec![if self.reaction {
                PortDescriptor::new(
                    PortId::try_new("in").expect("constant port"),
                    PortDirection::Input,
                    QueryChangeCodec::schema().descriptor().clone(),
                    PipeRequirements::default(),
                )
            } else {
                PortDescriptor::new(
                    PortId::try_new("out").expect("constant port"),
                    PortDirection::Output,
                    GraphChangeCodec::schema().descriptor().clone(),
                    PipeRequirements::default(),
                )
            }],
            completion: self
                .reaction
                .then_some(drasi_lib::computation::v1::SinkCompletion::Accepted),
            capabilities: Capabilities {
                control: !self.reaction,
                snapshot: self.reaction,
                ..Capabilities::default()
            },
        }
    }
    fn create(&self, request: &CreateRequest, _control: ControlSender) -> Result<CreatedComponent> {
        let descriptor = self.metadata().descriptor(request.id.clone())?;
        Ok(if self.reaction {
            Component::Sink(Box::new(reaction::Reaction::new(
                descriptor,
                request.configuration.clone(),
                self.hub.clone(),
            )?))
            .into()
        } else {
            CreatedComponent {
                component: Component::Source(Box::new(status::Producer::new(
                    descriptor,
                    request.configuration.clone(),
                    self.hub.clone(),
                )?)),
                control_handler: Some(Arc::new(lifecycle::Handler {
                    signals: None,
                    hub: self.hub.clone(),
                })),
            }
        })
    }
}
#[cfg(feature = "dynamic-plugin")]
drasi_computation_plugin_sdk::export_computation_plugin!(plugin());
#[cfg(test)]
mod tests;
