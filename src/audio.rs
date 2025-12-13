use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use itertools::Itertools;
use libspa::pod::deserialize::{ArrayPodDeserializer, PodDeserializer};
use libspa::utils::{Id, SpaTypes};
use pipewire;

use pipewire::context::{Context, ContextBox, ContextRc};
use pipewire::device::Device;
use pipewire::link::Link;
use pipewire::loop_::Signal;
use pipewire::main_loop::{MainLoop, MainLoopBox, MainLoopRc};
use pipewire::metadata::Metadata;
use pipewire::node::Node;
use pipewire::port::Port;
use pipewire::proxy::{Listener, ProxyT};
use pipewire::spa::param::ParamType;

use libspa::pod::{Pod, Value, ValueArray};
use pipewire::proxy::ProxyListener;
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

struct Proxies {
    proxies_t: HashMap<u32, Box<dyn ProxyT>>,
    listeners: HashMap<u32, Vec<Box<dyn Listener>>>,
}

impl Proxies {
    fn new() -> Self {
        Self {
            proxies_t: HashMap::new(),
            listeners: HashMap::new(),
        }
    }

    fn add_proxy_t(&mut self, proxy_t: Box<dyn ProxyT>, listener: Box<dyn Listener>) {
        let proxy_id = {
            let proxy = proxy_t.upcast_ref();
            proxy.id()
        };

        self.proxies_t.insert(proxy_id, proxy_t);

        let v = self.listeners.entry(proxy_id).or_default();
        v.push(listener);
    }

    fn add_proxy_listener(&mut self, proxy_id: u32, listener: ProxyListener) {
        let v = self.listeners.entry(proxy_id).or_default();
        v.push(Box::new(listener));
    }

    fn remove(&mut self, proxy_id: u32) {
        self.proxies_t.remove(&proxy_id);
        self.listeners.remove(&proxy_id);
    }
}

fn audio_generator(output: Sender<Message>, rt: Handle) -> Result<(), AudioError> {
    let mainloop = MainLoopRc::new(None)?;
    let mainloop_weak = mainloop.downgrade();
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let mainloop_weak = mainloop.downgrade();

    let _listener = core
        .add_listener_local()
        .info(|info| {
            dbg!(info);
        })
        .done(|_id, _seq| {})
        .error(move |id, seq, res, message| {
            log::error!("id: {id}, seq: {seq}, res: {res}, message: {message}");
            if id == 0 {
                if let Some(mainloop) = mainloop_weak.upgrade() {
                    mainloop.quit();
                }
            }
        });
    let registry = core.get_registry_rc()?;
    let registry_weak = registry.downgrade();
    let proxies = Rc::new(RefCell::new(Proxies::new()));
    let default_sink = Rc::new(RefCell::new(None));
    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            if let Some(registry) = registry_weak.upgrade() {
                use pipewire::types::ObjectType;
                let p: Option<(Box<dyn ProxyT>, Box<dyn Listener>)> = match global.type_ {
                    ObjectType::Node => {
                        let node: Node = registry.bind(global).unwrap();
                        let output = output.clone();
                        let obj_listener = node
                            .add_listener_local()
                            .info(|info| {
                                dbg!(info);
                            })
                            .param(move |_seq, param_type, _index, _next, param| {
                                match param_type {
                                    ParamType::Props => {}
                                    _ => unreachable!(),
                                }
                                let param_object = match param.map(Pod::as_object) {
                                    Some(v) => v,
                                    None => unreachable!(),
                                };
                                let param_object = match param_object {
                                    Ok(v) => v,
                                    Err(e) => {
                                        log::error!("{}", e);
                                        unreachable!();
                                    }
                                };
                                let volume_prop =
                                    if let Some(volume_prop) = param_object.find_prop(Id(65544)) {
                                        volume_prop
                                    } else {
                                        return;
                                    };
                                let volume_bytes = volume_prop.value().as_bytes();

                                let volume_value = if let Ok((_, value)) =
                                    PodDeserializer::deserialize_from::<Value>(volume_bytes)
                                {
                                    value
                                } else {
                                    return;
                                };
                                let volume_array = match volume_value {
                                    Value::ValueArray(v) => v,
                                    _ => unreachable!(),
                                };
                                let volume_float_array = match volume_array {
                                    ValueArray::Float(v) => v,
                                    _ => unreachable!(),
                                };
                                if let Err(e) = output.blocking_send(Message::Audio(
                                    AudioMessage::SinkVolume(volume_float_array),
                                )) {
                                    log::error!("Audio Error: {:?}", e);
                                };
                            })
                            .register();
                        node.subscribe_params(&[ParamType::Props]);
                        node.enum_params(0, None, 0, u32::MAX);
                        Some((Box::new(node), Box::new(obj_listener)))
                    }
                    ObjectType::Port => {
                        let port: Port = registry.bind(global).unwrap();
                        let port_listener = port
                            .add_listener_local()
                            .info(|info| {
                                dbg!(info);
                            })
                            .param(|seq, param_type, index, next, param| {
                                dbg!((seq, param_type, index, next, param.map(Pod::as_bytes)));
                            })
                            .register();
                        Some((Box::new(port), Box::new(port_listener)))
                    }
                    ObjectType::Link => {
                        let link: Link = registry.bind(global).unwrap();
                        let link_listener = link
                            .add_listener_local()
                            .info(|info| {
                                dbg!(info);
                            })
                            .register();
                        Some((Box::new(link), Box::new(link_listener)))
                    }
                    ObjectType::Metadata => {
                        let metadata: Metadata = registry.bind(global).unwrap();
                        let default_sink = default_sink.clone();
                        let metadata_listener = metadata
                            .add_listener_local()
                            .property(move |seq, key, metadata_type, value| {
                                if let Some(("default.audio.sink", value)) = key.zip(value.clone())
                                {
                                    let value = value.split_terminator("\"").nth(3);
                                    if let Some(value) = value {
                                        let value = value.to_string();
                                        default_sink.replace(Some(value));
                                    }
                                    dbg!(&default_sink);
                                }
                                dbg!((seq, key, metadata_type, value));
                                0
                            })
                            .register();
                        Some((Box::new(metadata), Box::new(metadata_listener)))
                    }
                    ObjectType::Device => {
                        let device: Device = registry.bind(global).unwrap();
                        let device_listener = device
                            .add_listener_local()
                            .info(|info| {
                                dbg!(info);
                            })
                            .param(|seq, param_type, a, b, value| {
                                dbg!((seq, param_type, a, b, value.map(Pod::as_bytes)));
                            })
                            .register();
                        device.subscribe_params(&[ParamType::Props, ParamType::Meta]);
                        device.enum_params(0, None, 0, u32::MAX);
                        Some((Box::new(device), Box::new(device_listener)))
                    }
                    ObjectType::Client
                    | ObjectType::Module
                    | ObjectType::Factory
                    | ObjectType::Other(_)
                    | ObjectType::Profiler
                    | ObjectType::Core => None,
                    _ => {
                        dbg!(global);
                        None
                    }
                };

                if let Some((proxy_spe, listener_spe)) = p {
                    let proxy = proxy_spe.upcast_ref();
                    let proxy_id = proxy.id();
                    // Use a weak ref to prevent references cycle between Proxy and proxies:
                    // - ref on proxies in the closure, bound to the Proxy lifetime
                    // - proxies owning a ref on Proxy as well
                    let proxies_weak = Rc::downgrade(&proxies);

                    let listener = proxy
                        .add_listener_local()
                        .removed(move || {
                            if let Some(proxies) = proxies_weak.upgrade() {
                                proxies.borrow_mut().remove(proxy_id);
                            }
                        })
                        .register();

                    proxies.borrow_mut().add_proxy_t(proxy_spe, listener_spe);
                    proxies.borrow_mut().add_proxy_listener(proxy_id, listener);
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
