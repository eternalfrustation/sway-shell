use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use libspa::pod::deserialize::PodDeserializer;
use pipewire;

use pipewire::client::Client;
use pipewire::context::Context;
use pipewire::device::Device;
use pipewire::link::Link;
use pipewire::main_loop::MainLoop;
use pipewire::metadata::Metadata;
use pipewire::node::Node;
use pipewire::port::Port;
use pipewire::properties::properties;
use pipewire::proxy::{Listener, ProxyT};
use pipewire::spa::param::ParamType;

use libspa::param::format::{MediaSubtype, MediaType};
use libspa::param::format_utils;
use libspa::pod::{Pod, Value};
use tokio::runtime::Handle;
use tokio::sync::RwLock;
use tokio::sync::mpsc::{Sender, channel};

use crate::state::Message;

#[derive(Debug)]
enum AudioError {
    PipewireError(pipewire::Error),
}

impl From<pipewire::Error> for AudioError {
    fn from(value: pipewire::Error) -> Self {
        Self::PipewireError(value)
    }
}

#[derive(Debug, Clone, Default)]
pub struct AudioState {
    pub sink_volume: Vec<f32>,
    pub source_volume: Vec<f32>,
}

#[derive(Debug)]
pub enum AudioMessage {
    SinkVolume(Vec<f32>),
    SourceVolume(Vec<f32>),
}

fn audio_generator(output: Sender<Message>, rt: Handle) -> Result<(), AudioError> {
    let mainloop = MainLoop::new(None)?;
    let context = Context::new(&mainloop)?;
    let core = context.connect(None)?;
    let registry = Rc::new(core.get_registry()?);
    let core = Rc::new(core);
    let registry_weak = Rc::downgrade(&registry);

    let persistence_store = Rc::new(RwLock::new(
        Vec::<(Box<dyn ProxyT>, Box<dyn Listener>)>::new(),
    ));
    let default_sink = Rc::new(RwLock::new(None));
    let default_sink_weak = Rc::downgrade(&default_sink);

    let default_sink_node = Rc::new(RwLock::new(None));

    let _registry_listener = registry
        .add_listener_local()
        .global(move |obj| {
            let registry = match registry_weak.upgrade() {
                Some(x) => x,
                None => return,
            };

            match &obj.type_ {
                pipewire::types::ObjectType::Metadata => {
                    let metadata: Metadata = registry.bind(obj).unwrap();
                    let default_sink_weak = default_sink_weak.clone();
                    let core = core.clone();
                    let obj_listener = metadata
                        .add_listener_local()
                        .property(move |_seq, key, _type_, value| {
                            let value = value.map(|v| String::from(v));
                            if let Some(default_sink) = default_sink_weak.upgrade() {
                                if let Some("default.audio.sink") = key {
                                    let mut default_sink = default_sink.blocking_write();
                                    if *default_sink != value {
                                        *default_sink = value;
                                        drop(default_sink);
                                        core.sync(1).unwrap();
                                    }
                                };
                            };
                            0
                        })
                        .register();
                    persistence_store
                        .blocking_write()
                        .push((Box::new(metadata), Box::new(obj_listener)));
                }
                pipewire::types::ObjectType::Node => {
                    let node: Node = registry.bind(obj).unwrap();
                    if let Some(Some(name)) = obj.props.map(|props| {
                        props
                            .iter()
                            .find_map(|(key, value)| (key == "node.name").then_some(value))
                    }) {
                        let default_sink_weak = default_sink_weak.clone();
                        let default_sink = if let Some(default_sink) = default_sink_weak.upgrade() {
                            default_sink
                        } else {
                            return;
                        };
                        dbg!(name, &default_sink);
                        let default_sink_name =
                            if let Some(default_sink_name) = default_sink.blocking_read().clone() {
                                default_sink_name
                            } else {
                                return;
                            };
                        if default_sink_name != name {
                            return;
                        }
                    } else {
                        return;
                    };
                    node.subscribe_params(&[ParamType::Props]);
                    let output = output.clone();
                    let obj_listener = node
                        .add_listener_local()
                        .param(move |_seq, _id, _index, _next, param| {
                            let params =
                                param.map(|v| PodDeserializer::deserialize_any_from(v.as_bytes()));
                            let params = match params {
                                Some(v) => v,
                                None => return,
                            }
                            .ok();
                            let params = match params {
                                Some(v) => v.1,
                                None => return,
                            };
                            let params = if let libspa::pod::Value::Object(params) = params {
                                params
                            } else {
                                return;
                            };

                            let volume = if let Some(volume) =
                                params.properties.iter().find_map(|v| {
                                    if v.key == libspa::sys::SPA_PROP_channelVolumes {
                                        Some(v.value.clone())
                                    } else {
                                        None
                                    }
                                }) {
                                volume
                            } else {
                                return;
                            };
                            let volume =
                                if let Value::ValueArray(libspa::pod::ValueArray::Float(volume)) =
                                    volume
                                {
                                    volume
                                } else {
                                    return;
                                };
                            output
                                .blocking_send(Message::Audio(AudioMessage::SinkVolume(volume)))
                                .unwrap();
                        })
                        .register();
                    default_sink_node
                        .blocking_write()
                        .replace((node, obj_listener));
                }

                x => {
                    log::info!("Pipewire message, don't care: {x:?}");
                }
            }
        })
        .register();
    mainloop.run();
    Ok(())
}
pub fn audio_subscription(rt: Handle) -> tokio_stream::wrappers::ReceiverStream<Message> {
    let (sender, receiver) = channel(1);

    rt.clone().spawn_blocking(move || {

        loop {
            log::error!(
                "Pipewire subscription event loop returned, this should never happen, trying to reconnect {:?}",
                audio_generator(sender.clone(), rt.clone())
            )
        }
    });
    tokio_stream::wrappers::ReceiverStream::new(receiver)
}
