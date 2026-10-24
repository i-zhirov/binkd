//! The outgoing calls, the port of `do_client()` and `call0()` of `client.c`.

use std::sync::Arc;

use binkd_config::model::Config;
use binkd_ftn::addr::FtnAddr;
use binkd_ftn::node::NP_ON;
use binkd_log::{DateTime, Log, ERROR, START};
use binkd_proto::{run_with, Report, Role, SendFile};
use binkd_spool::busy::{BusyList, F_BSY, F_CSY};
use binkd_spool::queue::Queue;
use binkd_spool::tryfile::{bad_try, good_try};
use tokio::net::TcpStream;

use crate::hosts::hosts;
use crate::session::{session_config, SessionOptions};

/// Returns the address family of a node, the port of `IP_afamily`.
fn family(node: Option<&binkd_ftn::node::FtnNode>) -> crate::srv::Family {
    match node.map(|node| node.ip_afamily) {
        Some(binkd_config::parser::AF_INET) => crate::srv::Family::V4,
        Some(binkd_config::parser::AF_INET6) => crate::srv::Family::V6,
        _ => crate::srv::Family::Unspecified,
    }
}

/// The caller failed.
#[derive(Debug)]
pub enum CallError {
    /// The transport failed.
    Io(std::io::Error),
    /// The node is busy.
    Busy,
    /// The hosts of the node hold no entry that the port can use.
    NoHost,
    /// The session failed.
    Session(binkd_proto::Error),
    /// The proxy failed.
    Proxy(crate::proxy::ProxyError),
}

/// One node to call, the port of the result of `q_next_node()`.
///
/// The C code holds a pointer to the node of the config; the port copies what a
/// call needs, so the caller holds no borrow of the config across an await.
#[derive(Clone, Debug)]
pub struct Call {
    /// The address of the node.
    pub fa: FtnAddr,
    /// The hosts of the node.
    pub hosts: String,
}

/// The caller of the outgoing calls, the port of the state of `client.c`.
///
/// The C code keeps the queue and the busy flags in the config and in process
/// wide values; the port keeps them in the caller, so a test drives one caller
/// and the binary drives another.
pub struct Caller {
    /// The busy flags of the calls, the port of `bsy_list` of `bsy.c`.
    busy: BusyList,
    /// The queue of the nodes to call, the port of the queue of the config.
    queue: Queue,
}

impl Default for Caller {
    fn default() -> Caller {
        Caller::new()
    }
}

impl Caller {
    /// Creates a caller, the port of the state of `client.c`.
    pub fn new() -> Caller {
        Caller {
            busy: BusyList::new(),
            queue: Queue::new(),
        }
    }

    /// Scans the spool, the port of the `q_scan()` of `do_client()`.
    ///
    /// The C code scans the spool when the flag of the config says that the
    /// queue is not there; the port scans on every call, which is the same
    /// result without the flag.
    pub fn scan(&mut self, config: &mut Config, now: i64) {
        self.queue = Queue::scan(&mut config.spool, now);
        // The port of the scan of `do_client()` with `SCAN_LISTED`: the scan
        // holds no queue entry and gives the flavours to the nodes.
        Queue::scan_listed(&mut config.spool, now);
    }

    /// Calls the next node, the port of `q_next_node()` and of `call()`.
    ///
    /// The port of `do_client()`: the busy flag of the node, the gate of the
    /// `max_clients` directive, and the try files. The call answers `None` when
    /// the queue holds no node that the caller may call.
    pub async fn call_next(
        &mut self,
        config: &mut Config,
        log: &Arc<Log>,
        now: &impl Fn() -> DateTime,
        options: SessionOptions,
        files: &impl Fn(&FtnAddr) -> Vec<SendFile>,
    ) -> Option<Result<Report, CallError>> {
        // The port of `do_client()`: the scan of the spool gives the flavours
        // to the nodes, the port of `q_scan(SCAN_LISTED, config)`. The C code
        // scans once for a pass and the port scans for every call, which is the
        // same result without the flag of the queue.
        self.scan(config, now().epoch);
        // The port of `q_next_node()` of `ftnq.c`: the node that is not busy,
        // that the config does not hold, and that the hold time passed. The
        // queue itself resets the flavours of the node and marks it busy.
        let call = {
            let node = Queue::next_node(&mut config.spool, now().epoch)?;
            Call {
                fa: node.fa.clone(),
                hosts: node.hosts.clone().unwrap_or_default(),
            }
        };
        // The port of the busy gate of `do_client()`: the C code skips the
        // node that holds the `.bsy` or the `.csy` flag.
        if !self.busy.test(&call.fa, F_BSY, &config.spool)
            || !self.busy.test(&call.fa, F_CSY, &config.spool)
        {
            log.message(
                START,
                &format!("{} busy, skipping", call.fa.format(false)),
                &now(),
            );
            return None;
        }
        // The port of `call()` of `client.c`: the `.csy` flag holds the call,
        // so a call and a session never run for one node at once.
        if !self.busy.add(&call.fa, F_CSY, &config.spool) {
            log.message(
                START,
                &format!("{} busy, skipping", call.fa.format(false)),
                &now(),
            );
            return None;
        }
        let result = self
            .call_hosts(&call, config, log, now, options, files)
            .await;
        self.busy.remove(&call.fa, F_CSY, &config.spool);
        // The port of the try files of `call0()`: a good session clears them,
        // and a bad one counts and holds the node.
        match &result {
            Ok(_) => good_try(&call.fa, "CONNECT/BND", &mut config.spool),
            Err(e) => {
                bad_try(&call.fa, &format!("{e:?}"), &mut config.spool, now().epoch);
            }
        }
        Some(result)
    }

    /// Calls the hosts of a node in turn, the port of `call0()` of `client.c`.
    ///
    /// The C code resolves the name of every host with `getaddrinfo()` and tries
    /// every address; the transport of the runtime resolves the name itself, so an
    /// entry that resolves to several addresses is one attempt for the port.
    async fn call_hosts(
        &self,
        call: &Call,
        config: &mut Config,
        log: &Arc<Log>,
        now: &impl Fn() -> DateTime,
        options: SessionOptions,
        files: &impl Fn(&FtnAddr) -> Vec<SendFile>,
    ) -> Result<Report, CallError> {
        let entries = hosts(&call.hosts, &call.fa, config);

        if entries.is_empty() {
            log.message(
                ERROR,
                &format!("{}: no usable host", call.fa.format(false)),
                &now(),
            );
            return Err(CallError::NoHost);
        }
        // The C code holds the node of the peer in a pointer of the config;
        // the port copies it, so the settings may borrow the config while the
        // node lives.
        let node = config.spool.nodes.get(&call.fa, now().epoch).cloned();
        // The port of `use_proxy` of `call0()`: the node asks for no proxy,
        // the node holds no pipe, and the config holds a proxy or a socks
        // server.
        let use_proxy = (!config.proxy.is_empty() || !config.socks.is_empty())
            && node.as_ref().map(|node| node.np_flag != NP_ON) == Some(true)
            && node
                .as_ref()
                .and_then(|node| node.pipe.as_deref())
                .unwrap_or("")
                .is_empty();
        // The C code resolves the proxy server once, before the loop of the
        // hosts, and it gives up when the name does not resolve.
        let proxy_addr = if use_proxy {
            Some(
                crate::proxy::proxy_address(
                    &config.proxy,
                    &config.socks,
                    family(node.as_ref()),
                    log,
                    now,
                )
                .await
                .map_err(CallError::Proxy)?,
            )
        } else {
            None
        };
        for entry in &entries {
            let addr = format!("{}:{}", entry.name, entry.port);
            let target = match &proxy_addr {
                Some(proxy) => {
                    log.message(
                        START,
                        &format!(
                            "trying {} via proxy {}:{}...",
                            entry.name,
                            proxy.ip(),
                            proxy.port()
                        ),
                        &now(),
                    );
                    *proxy
                }
                None => {
                    log.message(START, &format!("trying {addr}..."), &now());
                    // The port of the resolution of `call0()`: the C code asks
                    // the SRV lookup with the hints of the node.
                    let hints = crate::srv::Hints {
                        family: family(node.as_ref()),
                        ..crate::srv::Hints::default()
                    };
                    match crate::srv::srv_getaddrinfo(&entry.name, &entry.port, &hints).await {
                        Ok(addrs) => match addrs.first() {
                            Some(addr) => *addr,
                            None => continue,
                        },
                        Err(e) => {
                            log.message(
                                ERROR,
                                &format!("connection to {} failed: {e}", call.fa.format(false)),
                                &now(),
                            );
                            continue;
                        }
                    }
                }
            };
            // The port of the `connect()` of `call0()` with the timeout of
            // the `connect-timeout` directive.
            let mut stream =
                match tokio::time::timeout(options.timeout, TcpStream::connect(target)).await {
                    Ok(Ok(stream)) => stream,
                    Ok(Err(e)) => {
                        log.message(
                            ERROR,
                            &format!("connection to {} failed: {e}", call.fa.format(false)),
                            &now(),
                        );
                        continue;
                    }
                    Err(_) => {
                        log.message(
                            ERROR,
                            &format!("connection to {} timed out", call.fa.format(false)),
                            &now(),
                        );
                        continue;
                    }
                };
            log.message(START, &format!("connected to {addr}"), &now());
            // The port of `h_connect()` of `client.c`: the handshake of the
            // proxy leaves the socket as the tunnel to the peer.
            if proxy_addr.is_some() {
                if let Err(e) = crate::proxy::h_connect(
                    &mut stream,
                    &entry.name,
                    &entry.port,
                    config,
                    options.timeout,
                    log,
                    now,
                )
                .await
                {
                    log.message(
                        ERROR,
                        &format!("connection to {} failed: {e:?}", call.fa.format(false)),
                        &now(),
                    );
                    continue;
                }
            }
            let settings = session_config(config, node.as_ref(), options.crypt, options.trace);
            // The port of `protocol()`: the caller sends the files of the
            // queue for the peer, the port of the scan of `q_scan_boxes()`.
            let files = files(&call.fa);
            // The SRIF events of the session, the port of the calls of
            // `protocol.c` to `srif.c`.
            let mut srif = crate::srif::Events::new(config, target, Arc::clone(log), now());
            let result = run_with(
                stream,
                Role::Client,
                &settings,
                &mut |_| files.clone(),
                &mut |_| Ok(()),
                &mut srif,
                options.timeout,
            )
            .await;
            return result.map_err(CallError::Session);
        }
        Err(CallError::NoHost)
    }

    /// Calls the nodes of the queue, the port of `do_client()` of `client.c`.
    ///
    /// The C code branches a process for every call and waits for it; the port
    /// calls the nodes one after the other, the port of `call()` that waits for
    /// `call0()`. The gate of the `max_clients` directive counts the calls of
    /// one pass.
    pub async fn call_all(
        &mut self,
        config: &mut Config,
        log: &Arc<Log>,
        now: &impl Fn() -> DateTime,
        options: SessionOptions,
        files: &impl Fn(&FtnAddr) -> Vec<SendFile>,
    ) -> Vec<(FtnAddr, Result<Report, CallError>)> {
        self.scan(config, now().epoch);
        let mut out = Vec::new();
        let mut clients = 0;
        while clients < config.max_clients {
            let Some(node) = Queue::next_node(&mut config.spool, now().epoch) else {
                break;
            };
            let call = Call {
                fa: node.fa.clone(),
                hosts: node.hosts.clone().unwrap_or_default(),
            };
            if !self.busy.test(&call.fa, F_BSY, &config.spool)
                || !self.busy.test(&call.fa, F_CSY, &config.spool)
            {
                log.message(
                    START,
                    &format!("{} busy, skipping", call.fa.format(false)),
                    &now(),
                );
                continue;
            }
            if !self.busy.add(&call.fa, F_CSY, &config.spool) {
                continue;
            }
            clients += 1;
            let result = self
                .call_hosts(&call, config, log, now, options, files)
                .await;
            self.busy.remove(&call.fa, F_CSY, &config.spool);
            match &result {
                Ok(_) => good_try(&call.fa, "CONNECT/BND", &mut config.spool),
                Err(e) => {
                    bad_try(&call.fa, &format!("{e:?}"), &mut config.spool, now().epoch);
                }
            }
            out.push((call.fa, result));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use binkd_config::model::Config;

    #[tokio::test]
    async fn no_node_to_call() {
        let mut config = Config::new();
        let log = Arc::new(Log::new(0, 0, "", Vec::new()));
        let now = || DateTime {
            epoch: 0,
            day: 1,
            month: 1,
            hour: 0,
            minute: 0,
            second: 0,
        };
        let mut caller = Caller::new();
        assert!(caller
            .call_next(
                &mut config,
                &log,
                &now,
                SessionOptions::new(Duration::from_secs(1), true, false),
                &|_| Vec::new(),
            )
            .await
            .is_none());
    }
}
