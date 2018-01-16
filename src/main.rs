extern crate failure;
#[macro_use]
extern crate log;
extern crate stderrlog;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
#[macro_use]
extern crate serde_derive;
extern crate toml;

use std::env;
use std::fs::File;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use failure::{Error, Fail, ResultExt, err_msg};
use structopt::StructOpt;

fn tmux(args: Vec<&str>) -> Command {
    let mut cmd = Command::new("tmux");
    cmd.args(args);
    trace!("Executing: {:?}", cmd);
    cmd
}

#[derive(Debug, Deserialize)]
struct Session {
    name: Option<String>,
    root: Option<String>,
    pre_window: Option<String>,
    attach: Option<bool>,
    window: Vec<Window>,
}

impl Session {
    fn session_name(&self) -> Result<String, Error> {
        let name = match self.name.as_ref() {
            Some(n) => n.clone(),
            None => {
                env::current_dir()
                    .context("Failed to get current directory")?
                    .file_name()
                    .expect("Failed to get filename of current directory")
                    .to_os_string()
                    .into_string()
                    .expect("Failed to convert current directory name to string")
            }
        };
        Ok(name)
    }

    fn load(path: &Path) -> Result<Session, Error> {
        info!("Loading session");
        let mut file = File::open(path).context(
            "Unable to open configuration file",
        )?;
        let mut content = String::new();
        file.read_to_string(&mut content).context(
            "Unable to read configuration file",
        )?;
        toml::from_str(content.as_str()).map_err(|e| err_msg(e))
    }

    fn start(&self, dry_run: bool) -> Result<&Session, Error> {
        info!("Attempting to start session");
        if self.window.is_empty() {
            return Err(err_msg("Please configure at least one window."));
        }
        match tmux(vec!["has-session", "-t", self.session_name()?.as_str()]).status() {
            Ok(s) if (s.success()) => {
                Err(err_msg(
                    "Session already exists. Please explicitly set a unique name.",
                ))
            }
            Ok(_) => self.create(dry_run),
            Err(e) => Err(e.context("Error executing tmux"))?,
        }
    }

    fn create(&self, dry_run: bool) -> Result<&Session, Error> {
        info!("Creating session");
        let mut session_root = env::current_dir().context(
            "Failed to get current directory",
        )?;
        self.root.as_ref().map(|r| session_root.push(r));
        debug!("Session root: {:?}", session_root);
        let mut window_root = session_root.clone();
        self.window[0].root.as_ref().map(|r| window_root.push(r));
        let mut pane_root = window_root.clone();
        self.window[0].pane.get(0).as_ref().map(|p| {
            p.root.as_ref().map(|r| pane_root.push(r))
        });
        let session_name = self.session_name()?;
        let name = session_name.as_str();
        info!("Creating window 0");
        debug!("Root: {:?}", window_root);
        info!("Creating pane 0.0");
        debug!("Root: {:?}", pane_root);
        let mut cmd = tmux(vec![
            "new",
            "-d",
            "-s",
            name,
            "-c",
            pane_root.to_str().unwrap(),
        ]);
        if !dry_run {
            cmd.output().context("Error creating session")?;
        }
        for (i, window) in self.window.iter().enumerate() {
            window
                .create(dry_run, i, name, session_root.clone())
                .context("Error creating window")?;
        }
        let mut cmd = tmux(vec!["select-pane", "-t", format!("{}:0.0", name).as_str()]);
        if !dry_run {
            cmd.output().context("Error selecting pane")?;
        }
        if self.attach.unwrap_or(true) {
            let mut cmd = tmux(vec!["attach", "-t", name]);
            if !dry_run {
                cmd.exec();
            }
        }
        Ok(self)
    }

    fn kill(&self, dry_run: bool) -> Result<&Session, Error> {
        let mut cmd = tmux(vec!["kill-session", "-t", self.session_name()?.as_str()]);
        if !dry_run {
            cmd.output().context("Error killing session")?;
        }
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
struct Window {
    name: Option<String>,
    root: Option<String>,
    pane: Vec<Pane>,
}

impl Window {
    fn create(
        &self,
        dry_run: bool,
        index: usize,
        session_name: &str,
        session_root: PathBuf,
    ) -> Result<&Window, Error> {
        let mut window_root = session_root.clone();
        self.root.as_ref().map(|r| window_root.push(r));
        if index != 0 {
            info!("Creating window {}", index);
            debug!("Root: {:?}", window_root);
            info!("Creating pane {}.0", index);
            let mut pane_root = window_root.clone();
            self.pane.get(0).as_ref().map(|p| {
                p.root.as_ref().map(|r| pane_root.push(r))
            });
            debug!("Root: {:?}", pane_root);
            let mut cmd = tmux(vec![
                "new-window",
                "-t",
                session_name,
                "-c",
                pane_root.to_str().unwrap(),
            ]);
            if !dry_run {
                cmd.output().context("Failed to create new window")?;
            }
        }
        self.name.as_ref().map(|n| -> Result<(), Error> {
            let mut cmd = tmux(vec!["rename-window", "-t", session_name, n.as_str()]);
            if !dry_run {
                cmd.output().context("Failed to rename window")?;
            }
            Ok(())
        });
        for (i, pane) in self.pane.iter().enumerate() {
            pane.create(dry_run, index, i, session_name, window_root.clone())
                .context("Failed to create pane")?;
        }
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
struct Pane {
    root: Option<String>,
    command: Option<String>,
    split: Option<String>,
}

impl Pane {
    fn create(
        &self,
        dry_run: bool,
        window_index: usize,
        pane_index: usize,
        session_name: &str,
        window_root: PathBuf,
    ) -> Result<&Pane, Error> {
        if pane_index != 0 {
            info!("Creating pane {}.{}", window_index, pane_index);
            let mut pane_root = window_root.clone();
            self.root.as_ref().map(|r| pane_root.push(r));
            debug!("Root: {:?}", pane_root);
            let pane_name = format!("{}:{}", session_name, pane_index);
            let mut args = vec![
                "split-window",
                "-t",
                pane_name.as_str(),
                "-c",
                pane_root.to_str().expect(
                    "Failed to convert root directory name to string"
                ),
            ];
            self.split.as_ref().map(|s| if s == "horizontal" {
                args.push("-h");
            });
            let mut cmd = tmux(args);
            if !dry_run {
                cmd.output().context("Failed to create new pane")?;
            }
        }
        self.command.as_ref().map(|c| -> Result<(), Error> {
            let mut cmd = tmux(vec!["send-keys", format!("{}\n", c).as_str()]);
            if !dry_run {
                cmd.output().context("Failed to run command in pane")?;
            }
            Ok(())
        });
        Ok(&self)
    }
}

#[derive(StructOpt)]
#[structopt(version_short = "v")]
struct Opts {
    /// Configuration file
    #[structopt(long = "config", short = "c", default_value = ".tma.toml")]
    config: String,
    /// Dry run only, do not execute tmux commands
    #[structopt(long = "dry-run", short = "D")]
    dry_run: bool,
    /// Kill the configured session
    #[structopt(long = "kill", short = "k")]
    kill: bool,
    /// Increase verbosity
    #[structopt(long = "verbose", short = "V")]
    verbosity: u64,
}

fn main() {
    let args = Opts::from_args();
    stderrlog::new()
        //.module(module_path!())
        .verbosity(args.verbosity as usize)
        .init().unwrap();
    let path = Path::new(args.config.as_str());
    let session = Session::load(path).expect("Failed to load configuration");
    if args.kill {
        session.kill(args.dry_run).expect("Failed to kill session");
    } else {
        session.start(args.dry_run).expect(
            "Failed to start session",
        );
    }
}
