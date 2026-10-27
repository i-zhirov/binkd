//! The library form of binkd: a program embeds the engine of the port.
//!
//! The example binds the listener of a config and calls a node, the port of the
//! two managers of `main()` of `binkd.c` without the process of the daemon: the
//! caller creates the engine, holds the tasks, and ends on its own.

use std::sync::Arc;

use binkd::{Engine, FtnAddr};
use binkd_log::{Log, ERROR};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "binkd.conf".to_string());
    // The caller of the port parses the config, the port of `readcfg()`.
    let config = binkd_config::parse(&path)?;
    let log = Arc::new(Log::new(
        config.loglevel,
        config.conlog,
        &config.logpath,
        config.nolog.clone(),
    ));
    let mut engine = Engine::new(config, binkd::cli::Options::default(), log);
    // The listener of the engine runs in a task of the caller.
    let listener = engine.bind().await?;
    println!("the engine holds {} listener(s)", listener.len());
    // The caller names a node, the port of the `-P` switch of the C code.
    if let Some(node) = std::env::args().nth(2) {
        let fa = FtnAddr::parse(&node, &Default::default())
            .ok_or_else(|| format!("cannot parse the node {node}"))?;
        let report = engine.poll(&fa).await;
        println!("the engine holds the report of {fa:?}: {report:?}");
    }
    let _ = ERROR;
    Ok(())
}
