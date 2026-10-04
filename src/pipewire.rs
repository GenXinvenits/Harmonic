use std::sync::mpsc::Sender;
use std::thread;

#[derive(Debug, Clone, Copy)]
pub struct PipeWireSnapshot {
    pub connected: bool,
    pub node_count: usize,
}

pub fn spawn_discovery(tx: Sender<PipeWireSnapshot>) {
    thread::spawn(move || {
        pipewire::init();

        let mainloop = match pipewire::main_loop::MainLoopBox::new(None) {
            Ok(mainloop) => mainloop,
            Err(_) => {
                let _ = tx.send(PipeWireSnapshot {
                    connected: false,
                    node_count: 0,
                });
                return;
            }
        };

        let context = match pipewire::context::ContextBox::new(&mainloop.loop_(), None) {
            Ok(context) => context,
            Err(_) => {
                let _ = tx.send(PipeWireSnapshot {
                    connected: false,
                    node_count: 0,
                });
                return;
            }
        };

        let core = match context.connect(None) {
            Ok(core) => core,
            Err(_) => {
                let _ = tx.send(PipeWireSnapshot {
                    connected: false,
                    node_count: 0,
                });
                return;
            }
        };

        let registry = match core.get_registry() {
            Ok(registry) => registry,
            Err(_) => {
                let _ = tx.send(PipeWireSnapshot {
                    connected: false,
                    node_count: 0,
                });
                return;
            }
        };

        let node_count = std::rc::Rc::new(std::cell::Cell::new(0usize));
        let node_count_for_registry = node_count.clone();
        let tx_for_registry = tx.clone();

        let _listener = registry
            .add_listener_local()
            .global(move |global| {
                if global.type_ == pipewire::types::ObjectType::Node {
                    let next = node_count_for_registry.get() + 1;
                    node_count_for_registry.set(next);
                    let _ = tx_for_registry.send(PipeWireSnapshot {
                        connected: true,
                        node_count: next,
                    });
                }
            })
            .register();

        let _ = tx.send(PipeWireSnapshot {
            connected: true,
            node_count: node_count.get(),
        });

        mainloop.run();
    });
}
