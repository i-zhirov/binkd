//! The events of a session, the port of `evt_test()`, of `evt_queue()`, and of
//! `evt_set()` of `srif.c`.
//!
//! The C code tests every received file against the `flag` and `exec` entries of
//! the config. It creates an immediate flag file itself, it queues a delayed flag
//! file, and it answers 1 for a matching `exec` entry, so the caller runs the
//! command. The port answers the events, so the caller runs every effect.

use rbinkd_config::model::EvtFlag;
use rbinkd_ftn::pattern;

/// One event of a received file, the port of the two branches of `evt_test()`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Create the flag file of a `flag` entry.
    Flag {
        /// The path of the flag file.
        path: String,
        /// The C code creates the file at once, the port of `imm`.
        immediate: bool,
    },
    /// Run the command of an `exec` entry.
    Command {
        /// The command of the entry.
        command: String,
        /// The C code runs the command at once, the port of `imm`.
        immediate: bool,
    },
}

impl Event {
    /// Returns the path of the event, the port of the `path` of `EVTQ`.
    ///
    /// The C code holds the command of an `exec` entry in the same field, and it
    /// compares that field of two events of the same type.
    pub fn path(&self) -> &str {
        match self {
            Event::Flag { path, .. } => path,
            Event::Command { command, .. } => command,
        }
    }
}

/// Returns the events of a received file, the port of `evt_test()`.
///
/// The C code tests the entries of the config in order with the name of the file,
/// and it does not test the case. It creates an immediate flag file itself and it
/// answers 1 for a matching `exec` entry, so the caller runs the command.
pub fn events(flags: &[EvtFlag], filename: &str) -> Vec<Event> {
    let mut out = Vec::new();
    for flag in flags {
        if !pattern::pmatch_ncase(flag.pattern.as_bytes(), filename.as_bytes()) {
            continue;
        }
        if let Some(path) = &flag.path {
            out.push(Event::Flag {
                path: path.clone(),
                immediate: flag.imm,
            });
        } else if let Some(command) = &flag.command {
            out.push(Event::Command {
                command: command.clone(),
                immediate: flag.imm,
            });
        }
    }
    out
}

/// The events of a session that wait for the end of the session, the port of the
/// queue of `evt_queue()` and of the loop of `evt_set()`.
#[derive(Clone, Debug, Default)]
pub struct Events {
    /// The events, the newest first, the port of the list of the C code.
    queue: Vec<Event>,
}

impl Events {
    /// Adds an event, the port of `evt_queue()`.
    ///
    /// The C code holds one copy of an event of the same type and path, and it
    /// holds the newest event at the head of the list.
    pub fn add(&mut self, event: Event) {
        let duplicate = self.queue.iter().any(|queued| {
            std::mem::discriminant(queued) == std::mem::discriminant(&event)
                && queued.path() == event.path()
        });
        if duplicate {
            return;
        }
        self.queue.insert(0, event);
    }

    /// Returns the events and empties the queue, the port of the loop of
    /// `evt_set()`.
    pub fn drain(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.queue)
    }

    /// Reports whether the queue holds no event.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(path: &str, imm: bool) -> EvtFlag {
        EvtFlag {
            path: Some(path.to_string()),
            command: None,
            pattern: "*.pkt".to_string(),
            imm,
        }
    }

    fn command(command: &str, imm: bool) -> EvtFlag {
        EvtFlag {
            path: None,
            command: Some(command.to_string()),
            pattern: "*.req".to_string(),
            imm,
        }
    }

    #[test]
    fn the_events_of_a_file() {
        // The two entries of the C code hold a pattern each, and a file answers
        // the event of every entry whose pattern matches it.
        let flags = vec![
            flag("/var/spool/fido/toss.now", false),
            command("toss *F", true),
        ];
        assert_eq!(
            events(&flags, "13b70064.pkt"),
            vec![Event::Flag {
                path: "/var/spool/fido/toss.now".to_string(),
                immediate: false,
            }]
        );
        assert_eq!(
            events(&flags, "13b70064.req"),
            vec![Event::Command {
                command: "toss *F".to_string(),
                immediate: true,
            }]
        );
        // The C code does not test the case.
        assert_eq!(events(&flags, "13b70064.PKT").len(), 1);
        // A file that matches no pattern holds no event.
        assert!(events(&flags, "13b70064.mo").is_empty());
    }

    #[test]
    fn the_queue_of_the_events() {
        let mut queue = Events::default();
        assert!(queue.is_empty());
        queue.add(Event::Flag {
            path: "toss.now".to_string(),
            immediate: false,
        });
        // The C code holds one copy of an event of the same type and path.
        queue.add(Event::Flag {
            path: "toss.now".to_string(),
            immediate: false,
        });
        assert_eq!(queue.drain().len(), 1);
        // The C code holds a command and a flag of the same path apart.
        let mut queue = Events::default();
        queue.add(Event::Flag {
            path: "toss.now".to_string(),
            immediate: false,
        });
        queue.add(Event::Command {
            command: "toss.now".to_string(),
            immediate: false,
        });
        assert_eq!(queue.drain().len(), 2);
    }

    #[test]
    fn the_order_of_the_queue() {
        // The C code holds the newest event at the head of the list, so the
        // events of the end of the session run in the reverse order.
        let mut queue = Events::default();
        queue.add(Event::Command {
            command: "first".to_string(),
            immediate: false,
        });
        queue.add(Event::Command {
            command: "second".to_string(),
            immediate: false,
        });
        assert_eq!(
            queue.drain(),
            vec![
                Event::Command {
                    command: "second".to_string(),
                    immediate: false,
                },
                Event::Command {
                    command: "first".to_string(),
                    immediate: false,
                },
            ]
        );
        // The queue is empty after the drain.
        assert!(queue.is_empty());
        assert!(queue.drain().is_empty());
    }
}
