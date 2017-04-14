#[macro_use]
extern crate serde_derive;
extern crate toml;

use std::env;
use std::error::Error;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Write;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process;
use std::process::Command;

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
            env::current_dir().unwrap()
                .file_name().unwrap()
                .to_os_string().into_string().unwrap()
        }
    };
    let has_session = tmux(vec!["has-session", "-t", name.as_str()])
        .status();
    match has_session {
        Ok(s) if(s.success()) => {
            writeln!(io::stderr(), "Session already exists. Please explicitly set a unique name.").unwrap();
        },
        Ok(_) => { create_session(session, name); },
        Err(e) => { writeln!(io::stderr(), "Error executing tmux: {}", e.description()).unwrap(); }
    };
}

fn create_session(session: Session, name: String) {
    if session.window.is_empty() {
        writeln!(io::stderr(), "Please configure at least one window.").unwrap();
        process::exit(1);
    }
    let mut cmd = tmux(vec!["new", "-d", "-s", name.as_str()]);
    session.root.as_ref().map(|root| {
        let mut r = env::current_dir().unwrap();
        r.push(root);
        cmd.args(vec!["-c", r.to_str().unwrap()]);
    });
    session.window[0].pane.get(0).map(|first_pane| {
        first_pane.command.as_ref().map(|c| {
            cmd.arg(c.as_str())
        });
    });
    match cmd.output() {
        Err(e) => { writeln!(io::stderr(), "Error creating session: {}", e.description()).unwrap(); },
        Ok(_) => {
            //thread::sleep(Duration::from_millis(1000));
            session.window[0].name.as_ref().map(|n| {
                tmux(vec!["rename-window", "-t", name.as_str(), n.as_str()]).spawn().unwrap();
            });
            create_panes(&name, &session.window[0], 0);
            for (i, window) in session.window[1..].iter().enumerate() {
                let window_id = i+1;
                let mut cmd = tmux(vec!["new-window", "-t", name.as_str()]);
                window.name.as_ref().map(|n| {
                    cmd.args(vec!["-n", n.as_str()]);
                });
                window.root.as_ref().map(|root| {
                    let mut r = env::current_dir().unwrap();
                    r.push(root);
                    cmd.args(vec!["-c", r.to_str().unwrap()]);
                });
                window.pane.get(0).as_ref().map(|first_pane| {
                    first_pane.command.as_ref().map(|c| {
                        cmd.arg(c.as_str());
                    });
                });
                cmd.spawn().unwrap();
                create_panes(&name, window, window_id);
            }
        }
    };
    tmux(vec!["select-pane", "-t", format!("{}:0.0", name).as_str()]).spawn().unwrap();
    let should_attach = session.attach.unwrap_or(true);
    if should_attach {
        tmux(vec!["attach", "-t", name.as_str()]).exec();
    }
}

fn create_panes(name: &String, window: &Window, index: usize) {
    for pane in &window.pane[1..] {
        let mut cmd = tmux(vec!["split-window", "-t", format!("{}:{}", name, index).as_str()]);
        pane.root.as_ref().map(|root| {
            let mut r = env::current_dir().unwrap();
            r.push(root);
            cmd.args(vec!["-c", r.to_str().unwrap()]);
        });
        pane.command.as_ref().map(|c| {
            cmd.arg(c);
        });
        cmd.spawn().unwrap();
    }
}

fn main() {
    let path = Path::new(".tma.toml");
    match load(path) {
        Ok(session) => start(session),
        Err(e) => {
            writeln!(io::stderr(), "Error loading {}: {}", path.display(), e).unwrap();
        },
    };
}
