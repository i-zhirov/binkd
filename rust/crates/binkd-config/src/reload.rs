//! The reload of the config, the port of `checkcfg()` of `readcfg.c`.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::keywords::Features;
use crate::model::Config;
use crate::parser::{Error, Parser};
use crate::port;

/// The result of a check of the config files.
#[derive(Clone, Debug)]
pub enum Outcome {
    /// No file changed.
    Unchanged,
    /// A file changed: the new config is ready.
    Reloaded(Box<Config>),
    /// A file changed and the new config is broken. The old config stays.
    Failed(Error),
}

/// The reload of the config.
///
/// The C code keeps the config of the process in the `current_config` global and
/// checks the files on a timer. The port owns the config and returns the new value.
pub struct Reloader {
    config: Config,
    /// The `-C` option: check the files on every call.
    check_on_change: bool,
    /// The `SIGHUP` flag of the last call.
    forced: bool,
    /// The directory of the relative paths of the config.
    base_dir: Option<PathBuf>,
    /// The build features of the parse.
    features: Features,
    /// The time of the parse.
    now: i64,
}

impl Reloader {
    /// Creates a reloader for a config.
    pub fn new(config: Config) -> Reloader {
        Reloader {
            config,
            check_on_change: false,
            forced: false,
            base_dir: None,
            features: Features::unix_default(),
            now: 0,
        }
    }

    /// Sets the `-C` option: check the files on every call.
    pub fn set_check_on_change(&mut self, on: bool) {
        self.check_on_change = on;
    }

    /// Sets the `SIGHUP` flag of the next call.
    pub fn set_forced(&mut self, on: bool) {
        self.forced = on;
    }

    /// Sets the directory of the relative paths of the config.
    pub fn set_base_dir(&mut self, dir: &Path) {
        self.base_dir = Some(dir.to_path_buf());
    }

    /// Sets the build features of the parse.
    pub fn set_features(&mut self, features: Features) {
        self.features = features;
    }

    /// Sets the time of the parse.
    pub fn set_now(&mut self, now: i64) {
        self.now = now;
    }

    /// Returns the config in use.
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Returns the config in use for change.
    pub fn config_mut(&mut self) -> &mut Config {
        &mut self.config
    }

    /// Checks the config files and reloads the config.
    ///
    /// The port of `checkcfg()`. A file with a changed time starts the parse of
    /// the first file of the list, so the whole config is read again.
    pub fn check(&mut self) -> Outcome {
        let forced = std::mem::take(&mut self.forced);
        if !self.check_on_change && !forced {
            return Outcome::Unchanged;
        }

        let mut need_reload = forced;
        let base_dir = self.base_dir.clone();
        for file in &mut self.config.config_list {
            let full = match &base_dir {
                Some(dir) => dir.join(&file.path),
                None => PathBuf::from(&file.path),
            };
            let Ok(meta) = fs::metadata(&full) else {
                continue;
            };
            let mtime = meta
                .modified()
                .ok()
                .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            if file.mtime == 0 {
                // The first check of a file stores its time.
                file.mtime = mtime;
            } else if file.mtime != mtime {
                // The C code stores the new time, so a broken config does not
                // reload on every check.
                file.mtime = mtime;
                need_reload = true;
            }
        }

        if !need_reload {
            return Outcome::Unchanged;
        }

        // The C code reloads from the first file of the list.
        let Some(first) = self.config.config_list.first().map(|f| f.path.clone()) else {
            return Outcome::Unchanged;
        };

        let mut parser = Parser::with_ports(&port::resolve);
        parser.set_features(self.features);
        parser.set_now(self.now);
        if let Some(dir) = &self.base_dir {
            parser.set_base_dir(dir);
        }
        match parser.parse(&first) {
            Ok(config) => {
                self.config = config.clone();
                Outcome::Reloaded(Box::new(config))
            }
            Err(e) => Outcome::Failed(e),
        }
    }
}
