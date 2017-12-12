#![recursion_limit = "1024"]
#[macro_use]
extern crate error_chain;
extern crate structopt;
#[macro_use]
extern crate structopt_derive;
#[macro_use]
extern crate serde_derive;
extern crate toml;

use std::env;
use std::error::Error;
use std::fs::File;
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use structopt::StructOpt;

mod errors {
    error_chain!{}
}

use errors::*;

fn tmux(args: Vec<&str>) -> Command {
    let mut cmd = Command::new("tmux");
    cmd.args(args);
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
    fn session_name(&self) -> Result<String> {
        let name = match self.name.as_ref() {
            Some(n) => n.clone(),
            None => {
                env::current_dir()
                    .chain_err(|| "Failed to get current directory")?
                    .file_name()
                    .chain_err(|| "Failed to get filename of current directory")?
                    .to_os_string()
                    .into_string()
                    .expect("Failed to convert current directory name to string")
            }
        };
        Ok(name)
    }

    fn load(path: &Path) -> Result<Session> {
        let mut file = File::open(path).chain_err(
            || "Unable to open configuration file",
        )?;
        let mut content = String::new();
        file.read_to_string(&mut content).chain_err(
            || "Unable to read configuration file",
        )?;
        toml::from_str(content.as_str()).chain_err(|| "Unable to load configuration")
    }

    fn start(&self) -> Result<&Session> {
        match tmux(vec!["has-session", "-t", self.session_name()?.as_str()]).status() {
            Ok(s) if (s.success()) => {
                Err(
                    "Session already exists. Please explicitly set a unique name.".into(),
                )
            }
            Ok(_) => self.create(),
            Err(e) => Err(format!("Error executing tmux: {}", e.description()).into()),
        }
    }

    fn create(&self) -> Result<&Session> {
        let name = self.session_name()?;
        if self.window.is_empty() {
            return Err("Please configure at least one window.".into());
        }
        let mut session_root = env::current_dir().chain_err(
            || "Failed to get current directory",
        )?;
        self.root.as_ref().map(|r| session_root.push(r));
        let mut root = session_root.clone();
        self.window[0].root.as_ref().map(|r| root.push(r));
        self.window[0].pane.get(0).as_ref().map(|p| {
            p.root.as_ref().map(|r| root.push(r))
        });
        tmux(vec![
            "new",
            "-d",
            "-s",
            name.as_str(),
            "-c",
            root.to_str().unwrap(),
        ]).output()
            .chain_err(|| "Error creating session")?;
        for (i, window) in self.window.iter().enumerate() {
            if i != 0 {
                let mut window_root = session_root.clone();
                window.root.as_ref().map(|root| window_root.push(root));
                window.pane.get(0).as_ref().map(|p| {
                    p.root.as_ref().map(|r| window_root.push(r))
                });
                let mut cmd = tmux(vec![
                    "new-window",
                    "-t",
                    name.as_str(),
                    "-c",
                    window_root.to_str().unwrap(),
                ]);
                cmd.output().chain_err(|| "Failed to create new window")?;
            }
            window.name.as_ref().map(|n| -> Result<()> {
                tmux(vec!["rename-window", "-t", name.as_str(), n.as_str()])
                    .output()
                    .chain_err(|| "Failed to rename window")?;
                Ok(())
            });
            self.create_panes(window, i)?;
        }
        tmux(vec!["select-pane", "-t", format!("{}:0.0", name).as_str()])
            .output()
            .chain_err(|| "Error selecting pane")?;
        if self.attach.unwrap_or(true) {
            tmux(vec!["attach", "-t", name.as_str()]).exec();
        }
        Ok(self)
    }

    fn create_panes(&self, window: &Window, index: usize) -> Result<&Session> {
        let name = self.session_name()?;
        let mut root = env::current_dir().chain_err(
            || "Failed to get current directory",
        )?;
        self.root.as_ref().map(|r| root.push(r));
        window.root.as_ref().map(|r| root.push(r));
        for (i, pane) in window.pane.iter().enumerate() {
            if i != 0 {
                let mut cmd = tmux(vec![
                    "split-window",
                    "-t",
                    format!("{}:{}", name, index).as_str(),
                ]);
                let mut pane_root = root.clone();
                pane.root.as_ref().map(|r| pane_root.push(r));
                cmd.args(vec![
                    "-c",
                    pane_root.to_str().chain_err(
                        || "Failed to convert root directory name to string"
                    )?,
                ]);
                pane.split.as_ref().map(|s| if s == "horizontal" {
                    cmd.arg("-h");
                });
                cmd.output().chain_err(|| "Failed to create new pane")?;
            }
            pane.command.as_ref().map(|c| -> Result<()> {
                tmux(vec!["send-keys", format!("{}\n", c).as_str()])
                    .output()
                    .chain_err(|| "Failed to run command in pane")?;
                Ok(())
            });
        }
        Ok(self)
    }

    fn kill(&self) -> Result<&Session> {
        let mut cmd = tmux(vec!["kill-session", "-t", self.session_name()?.as_str()]);
        cmd.output().chain_err(|| "Error killing session")?;
        Ok(self)
    }
}

#[derive(Debug, Deserialize)]
struct Window {
    name: Option<String>,
    root: Option<String>,
    pane: Vec<Pane>,
}

#[derive(Debug, Deserialize)]
struct Pane {
    root: Option<String>,
    command: Option<String>,
    split: Option<String>,
}

#[derive(StructOpt, Debug)]
#[structopt(name = "tma")]
struct Cli {
    /// Configuration file
    #[structopt(long = "config", short = "c", default_value = ".tma.toml")]
    config: String,
    /// Kill the configured session
    #[structopt(long = "kill", short = "k")]
    kill: bool,
}

quick_main!(|| -> Result<()> {
    let args = Cli::from_args();
    let path = Path::new(args.config.as_str());
    let session = Session::load(path)?;
    if args.kill {
        session.kill()?;
    } else {
        session.start()?;
    }
    Ok(())
});
