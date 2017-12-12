#![recursion_limit = "1024"]
#[macro_use] extern crate error_chain;
extern crate structopt;
#[macro_use] extern crate structopt_derive;
#[macro_use] extern crate serde_derive;
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
    error_chain! {
    }
}

use errors::*;

#[derive(Debug, Deserialize)]
struct Session {
    name: Option<String>,
    root: Option<String>,
    pre_window: Option<String>,
    attach: Option<bool>,
    window: Vec<Window>,
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

fn load(path: &Path) -> Result<Session> {
    let mut file = File::open(path)
        .chain_err(|| "Unable to open configuration file")?;
    let mut content = String::new();
    file.read_to_string(&mut content)
        .chain_err(|| "Unable to read configuration file")?;
    toml::from_str(content.as_str())
        .chain_err(|| "Unable to load configuration")
}

fn tmux(args: Vec<&str>) -> Command {
    let mut cmd = Command::new("tmux");
    cmd.args(args);
    cmd
}

fn session_name(session: &Session) -> Result<String> {
    let name = match session.name.as_ref() {
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

fn start(session: Session) -> Result<()> {
    let name = session_name(&session)?;
    match tmux(vec!["has-session", "-t", name.as_str()]).status() {
        Ok(s) if (s.success()) => {
            return Err("Session already exists. Please explicitly set a unique name.".into());
        }
        Ok(_) => {
            create_session(&session, name)
                .chain_err(|| "Unable to create session")?;
        }
        Err(e) => {
            return Err(format!("Error executing tmux: {}", e.description()).into());
        }
    };
    Ok(())
}

fn create_session(session: &Session, name: String) -> Result<()> {
    if session.window.is_empty() {
        return Err("Please configure at least one window.".into());
    }
    let mut session_root = env::current_dir().expect("Failed to get current directory");
    session.root.as_ref().map(|r| session_root.push(r));
    let mut root = session_root.clone();
    session.window[0].root.as_ref().map(|r| root.push(r));
    session.window[0].pane.get(0).as_ref().map(|p| p.root.as_ref().map(|r| root.push(r)));
    match tmux(vec!["new", "-d", "-s", name.as_str(), "-c", root.to_str().unwrap()]).output() {
        Err(e) => {
            return Err(format!("Error creating session: {}", e.description()).into());
        }
        Ok(_) => {
            for (i, window) in session.window.iter().enumerate() {
                if i != 0 {
                    let mut window_root = session_root.clone();
                    window.root.as_ref().map(|root| window_root.push(root));
                    window.pane
                          .get(0)
                          .as_ref()
                          .map(|p| p.root.as_ref().map(|r| window_root.push(r)));
                    let mut cmd = tmux(vec!["new-window",
                                            "-t",
                                            name.as_str(),
                                            "-c",
                                            window_root.to_str().unwrap()]);
                    cmd.output().expect("Failed to create new window");
                }
                window.name.as_ref().map(|n| -> Result<()> {
                    tmux(vec!["rename-window", "-t", name.as_str(), n.as_str()])
                        .output()
                        .chain_err(|| "Failed to rename window")?;
                        Ok(())
                });
                create_panes(session, &name, window, i);
            }
        }
    };
    tmux(vec!["select-pane", "-t", format!("{}:0.0", name).as_str()])
        .output()
        .chain_err(|| "Error running tmux")?;
    if session.attach.unwrap_or(true) {
        tmux(vec!["attach", "-t", name.as_str()]).exec();
    }
    Ok(())
}

fn create_panes(session: &Session, name: &String, window: &Window, index: usize) {
    let mut root = env::current_dir().expect("Failed to get current directory");
    session.root.as_ref().map(|r| root.push(r));
    window.root.as_ref().map(|r| root.push(r));
    for (i, pane) in window.pane.iter().enumerate() {
        if i != 0 {
            let mut cmd = tmux(vec!["split-window", "-t", format!("{}:{}", name, index).as_str()]);
            let mut pane_root = root.clone();
            pane.root.as_ref().map(|r| pane_root.push(r));
            cmd.args(vec!["-c",
                          pane_root.to_str()
                                   .expect("Failed to convert root directory name to string")]);
            pane.split.as_ref().map(|s| {
                if s == "horizontal" {
                    cmd.arg("-h");
                }
            });
            cmd.output().expect("Failed to create new pane");
        }
        pane.command.as_ref().map(|c| {
            tmux(vec!["send-keys", format!("{}\n", c).as_str()])
                .output()
                .expect("Failed to run command in pane");
        });
    }
}

fn kill(session: &Session) -> Result<std::process::Output> {
    let mut cmd = tmux(vec!["kill-session", "-t", session_name(session)?.as_str()]);
    cmd.output()
        .chain_err(|| "Error killing session")
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
    let session = load(path)
        .chain_err(|| "Unable to open configuration file")?;
    if args.kill {
        kill(&session)?;
    } else {
        start(session)?;
    }
    Ok(())
});
