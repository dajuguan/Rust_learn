/*
Minimal VS Code–Style Runtime Plugin Architecture (in Rust)

This design models VS Code’s runtime plugin system, stripped down to its core ideas and implemented in Rust without IPC.

Core properties
- Plugins are registered at runtime
- Plugins do not participate in the core control flow
- The core owns execution and dispatch
- Plugins extend behavior via callbacks
 */

use std::collections::HashMap;

// Type alias for command handler: single-dispatch, no args
type CommandHandler = Box<dyn Command>;
// Type alias for an event handler: can receive a payload (&str), fan-out
type EventHandler = Box<dyn Event>;
pub trait Command: Fn() + Send + Sync + 'static {}
pub trait Event: Fn(&str) + Send + Sync + 'static {}

// Implement the traits for all matching closures
impl<T> Command for T where T: Fn() + Send + Sync + 'static {}
impl<T> Event for T where T: Fn(&str) + Send + Sync + 'static {}

#[derive(Default)]
pub struct PluginHost {
    commands: HashMap<String, CommandHandler>,
    events: HashMap<String, Vec<EventHandler>>,
}

impl PluginHost {
    pub fn register_cmd(&mut self, name: &str, cmd: impl Command) -> Result<(), String> {
        let name = name.to_string();
        if self.commands.contains_key(&name) {
            Err(format!("Command {} is already registed", name))
        } else {
            self.commands.insert(name, Box::new(cmd));
            Ok(())
        }
    }

    pub fn register_events(&mut self, name: &str, handler: impl Event) {
        let name = name.to_string();
        self.events.entry(name).or_default().push(Box::new(handler));
    }

    pub fn execute_cmd(&self, name: &str) {
        if let Some(cmd) = self.commands.get(name) {
            cmd();
        }
    }

    pub fn emit_event(&self, name: &str, payload: &str) {
        if let Some(subscribers) = self.events.get(name) {
            for handler in subscribers {
                handler(payload)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_a(host: &mut PluginHost) {
        host.register_cmd("say_hello", || {
            println!("[Plugin A] Called");
        })
        .unwrap();

        host.register_events("event", |payload| {
            println!("[Plugin A] Got event: {}", payload);
        });
    }

    fn plugin_b(host: &mut PluginHost) {
        host.register_cmd("say_world", || {
            println!("[Plugin B] Called");
        })
        .unwrap();

        host.register_events("event", |payload| {
            println!("[Plugin B] Got event: {}", payload);
        });
    }

    #[test]
    fn test_plugins() {
        let mut host = PluginHost::default();
        plugin_a(&mut host);
        plugin_b(&mut host);

        host.execute_cmd("say_hello");
        host.emit_event("event", "random_event");
    }
}
