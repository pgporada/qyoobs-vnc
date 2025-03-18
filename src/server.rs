use std::{
    fs::File,
    io::{self},
    os::{fd::AsFd, unix::net::UnixStream},
    process::{self, Child, Command, Stdio},
    thread,
};

use anyhow::{Context, Error, anyhow};

use crate::sigkill;

pub fn run(args: Option<String>) -> Result<(), Error> {
    let region_selection = RegionSelection::parse(args)?;

    let mut args = Vec::from([
        "-viewonly".to_string(),
        "-inetd".to_string(),
        "-wait".to_string(),
        "200".to_string(),
        "-v".to_string(),
        // TODO(inahga): log to file
    ]);
    args.extend(region_selection.render());

    // It would be ideal if we could simply Stdio::inherit() stdin and stdout, but it seems x11vnc
    // doesn't play nice with pipes. We'll have to play games with sockets. See
    // https://github.com/LibVNC/x11vnc/issues/148.
    let (parent, child) = UnixStream::pair()?;

    let handle = Command::new("x11vnc")
        .stdin(child.as_fd().try_clone_to_owned()?)
        .stdout(child.as_fd().try_clone_to_owned()?)
        .stderr(Stdio::inherit())
        .args(args)
        .spawn()?;

    drop(child);

    let _watchdog = Watchdog::new(handle);

    let mut parent_input = parent;
    let mut parent_output = parent_input.try_clone()?;

    let input_thread = thread::spawn(move || io::copy(&mut io::stdin(), &mut parent_input));

    let output_thread = thread::spawn(move || {
        // We need stdout to be unbuffered. Grab its underlying file descriptor
        let mut stdout: File = io::stdout().as_fd().try_clone_to_owned()?.into();
        io::copy(&mut parent_output, &mut stdout)
    });

    input_thread.join().unwrap().context("input_thread")?;
    output_thread.join().unwrap().context("output_thread")?;

    Ok(())
}

enum RegionSelection {
    Display,
    Window(u32),
    Screen(u32),
}

impl RegionSelection {
    fn parse(input: Option<String>) -> Result<Self, Error> {
        match input {
            None => Ok(Self::Display),
            Some(input) => {
                if let Some((_, value)) = input.split_once("window") {
                    let value = value
                        .strip_prefix("0x")
                        .ok_or(anyhow!("invalid hexadecimal value"))?;
                    return Ok(Self::Window(u32::from_str_radix(value, 16)?));
                }
                if let Some((_, value)) = input.split_once("screen") {
                    return Ok(Self::Screen(value.parse()?));
                }
                Err(anyhow!("argument unrecognized"))
            }
        }
    }

    fn render(&self) -> Vec<String> {
        match self {
            RegionSelection::Display => Vec::new(),
            RegionSelection::Screen(screen) => {
                Vec::from(["-clip".to_string(), format!("xinerama{screen}")])
            }
            RegionSelection::Window(window) => {
                Vec::from(["-sid".to_string(), format!("0x{:x}", window)])
            }
        }
    }
}

struct Watchdog(u32);

impl Watchdog {
    fn new(mut child: Child) -> Self {
        let pid = child.id();
        thread::spawn(move || {
            let result = child.wait();
            match result {
                Ok(status) => eprintln!("x11vnc exited with {status}"),
                Err(err) => eprintln!("x11vnc exited: {err}"),
            }
            process::exit(420);
        });

        Self(pid)
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        // SIGKILL is rude, but I've found sometimes x11vnc gets stuck when the client disconnects
        // and requires ye olde kill -9.
        sigkill(self.0, false);
    }
}
