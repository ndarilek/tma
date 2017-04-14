extern crate getopts;
#[macro_use]
extern crate serde_derive;
extern crate toml;

use getopts::Options;
use std::env;
use std::error::Error;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process;
use std::process::{Command, Stdio};

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
    layout: Option<String>,
    pane: Vec<Pane>
}

#[derive(Debug, Deserialize)]
struct Pane {
    root: Option<String>,
    command: Option<String>,
}

fn load(path: &Path) -> Result<Session, String> {
    let mut file = try!(File::open(path).map_err(|e| e.to_string()));
    let mut content = String::new();
    try!(file.read_to_string(&mut content).map_err(|e| e.to_string()));
    let session: Result<Session, toml::de::Error> = toml::from_str(content.as_str());
    session.map_err(|e| e.to_string())
}

fn tmux(args: Vec<&str>) -> Command {
    let mut cmd= Command::new("tmux");
    cmd.args(args);
    cmd
}

fn start(session: Session) {
    let name = match session.name.as_ref() {
        Some(n) => n.clone(),
        None => {
            env::current_dir().expect("Failed to get current directory")
                .file_name().expect("Failed to get filename of current directory")
                .to_os_string().into_string().expect("Failed to convert current directory name to string")
        }
    };
    match tmux(vec!["has-session", "-t", name.as_str()]).status() {
        Ok(s) if(s.success()) => {
            writeln!(io::stderr(), "Session already exists. Please explicitly set a unique name.").expect("Failed to write to stderr");
        },
        Ok(_) => { create_session(&session, name); },
        Err(e) => { writeln!(io::stderr(), "Error executing tmux: {}", e.description()).expect("Failed to write to stderr"); }
    };
}

fn create_session(session: &Session, name: String) {
    if session.window.is_empty() {
        writeln!(io::stderr(), "Please configure at least one window.").expect("Failed to write to stderr");
        process::exit(1);
    }
    let mut cmd = tmux(vec!["new", "-d", "-s", name.as_str()]);
    let mut root = env::current_dir().expect("Failed to get current directory");
    session.root.as_ref().map(|r| root.push(r));
    let session_root = root.clone();
    session.window[0].pane.get(0).map(|first_pane| {
        first_pane.root.as_ref().map(|r| root.push(r));
        cmd.args(vec!["-c", root.to_str().expect("Failed to convert root directory name to string")]);
        first_pane.command.as_ref().map(|c|  cmd.arg(c.as_str()));
    });
    match cmd.output() {
        Err(e) => { writeln!(io::stderr(), "Error creating session: {}", e.description()).expect("Failed to write to stderr"); },
        Ok(_) => {
            session.window[0].name.as_ref().map(|n| {
                tmux(vec!["rename-window", "-t", name.as_str(), n.as_str()]).output().expect("Failed to rename window");
            });
            create_panes(session, &name, &session.window[0], 0);
            for (i, window) in session.window[1..].iter().enumerate() {
                let window_id = i+1;
                let mut cmd = tmux(vec!["new-window", "-t", name.as_str()]);
                window.name.as_ref().map(|n| cmd.args(vec!["-n", n.as_str()]));
                let mut window_root = session_root.clone();
                window.root.as_ref().map(|root| window_root.push(root));
                window.pane.get(0).as_ref().map(|first_pane| {
                    first_pane.root.as_ref().map(|r| window_root.push(r));
                    cmd.args(vec!["-c", window_root.to_str().expect("Failed to convert root directory name to string")]);
                    first_pane.command.as_ref().map(|c| cmd.arg(c.as_str()));
                });
                cmd.output().expect("Failed to create new window");
                create_panes(session, &name, window, window_id);
            }
        }
    };
    tmux(vec!["select-pane", "-t", format!("{}:0.0", name).as_str()]).output().expect("Failed to select initial window and pane");
    let should_attach = session.attach.unwrap_or(true);
    if should_attach {
        tmux(vec!["attach", "-t", name.as_str()]).exec();
    }
}

fn create_panes(session: &Session, name: &String, window: &Window, index: usize) {
    let mut root = env::current_dir().expect("Failed to get current directory");
    session.root.as_ref().map(|r| root.push(r));
    window.root.as_ref().map(|r| root.push(r));
    for pane in &window.pane[1..] {
        let mut cmd = tmux(vec!["split-window", "-t", format!("{}:{}", name, index).as_str()]);
        let mut pane_root = root.clone();
        pane.root.as_ref().map(|r| pane_root.push(r));
        cmd.args(vec!["-c", pane_root.to_str().expect("Failed to convert root directory name to string")]);
        pane.command.as_ref().map(|c| cmd.arg(c));
        cmd.output().expect("Failed to create new pane");
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();
    let mut opts = Options::new();
    opts.optopt("c", "", "specify configuration file (defaults to .tma.toml)", "FILE");
    opts.optflag("h", "help", "print this help text");
    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => panic!(f.to_string())
    };
    if matches.opt_present("h") {
        let brief = format!("Usage: {} [options]", program);
        print!("{}", opts.usage(&brief));
        return;
    }
    let path_str = matches.opt_str("c").unwrap_or(".tma.toml".to_string());
    let path = Path::new(path_str.as_str());
    match load(path) {
        Ok(session) => start(session),
        Err(e) => {
            writeln!(io::stderr(), "Error loading {}: {}", path.display(), e).expect("Failed to write to stderr");
        },
    };
}
