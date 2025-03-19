use std::process::Command;

use anyhow::{Error, anyhow};
use argh::FromArgs;
use dialoguer::Select;
use nanoserde::DeJson;
use probe::{Monitor, Probe, Window};

mod connect;
mod probe;
mod server;

const IDENT: &str = "qyoobs.VNC";
const PROBE: &str = "qyoobs.VNC+probe";

fn main() -> Result<(), Error> {
    let args: Args = argh::from_env();
    match args.subcommand {
        Subcommand::Server(sub) => server::run(sub.args),
        Subcommand::Connect(sub) => connect::run(args.verbose.unwrap_or_default(), sub),
        Subcommand::Probe(_) => probe::probe(),
    }
}

/// Opens a VNC session to another Qube.
#[derive(FromArgs)]
struct Args {
    #[argh(subcommand)]
    subcommand: Subcommand,

    /// debug logging
    #[argh(switch, short = 'v')]
    verbose: Option<bool>,
}

#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand)]
enum Subcommand {
    Connect(ConnectArgs),
    Server(ServerArgs),
    Probe(ProbeArgs),
}

/// Run the VNC server. You shouldn't normally need to run this yourself.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "server")]
pub struct ServerArgs {
    #[argh(positional)]
    args: String,
}

/// Open a VNC viewer to the target Qube.
///
/// If no arguments are provided, the entire display of the target Qube is captured.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "connect")]
pub struct ConnectArgs {
    /// name of the target VM whose screen will be captured
    #[argh(positional)]
    target_vmname: String,

    /// xinerama id of the monitor to capture
    #[argh(option, short = 'm', from_str_fn(parse_u32_from_str))]
    monitor: Option<u32>,

    /// id of the window to capture
    ///
    /// You can identify the target window's ID using `xwininfo -int` or
    /// `xwininfo -int -root -children` on the target qube.
    #[argh(option, short = 'w', from_str_fn(parse_u32_from_str))]
    window: Option<u32>,

    /// interactively choose a window or screen on the target qube to capture
    #[argh(switch, short = 'c')]
    choose: bool,
}

/// List windows on the current system. You shouldn't normally need to run this yourself.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "probe")]
pub struct ProbeArgs {}

impl ConnectArgs {
    pub fn target_vmname(&self) -> &str {
        &self.target_vmname
    }

    fn region_selection(&self) -> Result<String, Error> {
        match (self.monitor, self.window, self.choose) {
            (None, None, false) => Ok("display".to_string()),
            (Some(monitor), None, false) => Ok(format!("monitor{monitor}")),
            (None, Some(window), false) => Ok(format!("window{window}")),
            (None, None, true) => self.choose_region(),
            (_, _, _) => Err(anyhow!(
                "malformed arguments: --monitor, --window, and --chose are mutually exclusive"
            )),
        }
    }

    fn choose_region(&self) -> Result<String, Error> {
        eprintln!(
            "Enumerating screens and windows on the target qube. You will be prompted to allow this.",
        );
        let qrexec = Command::new("qrexec-client-vm")
            .args([self.target_vmname(), PROBE])
            .output()?;

        if !qrexec.status.success() {
            return Err(anyhow!("qrexec-client-vm exited with {}", qrexec.status));
        }

        let stdout = String::from_utf8(qrexec.stdout)?;
        let probe: Probe = DeJson::deserialize_json(&stdout)?;

        enum MenuItem {
            Display,
            Monitor(Monitor),
            Window(Window),
        }

        impl ToString for MenuItem {
            fn to_string(&self) -> String {
                match self {
                    MenuItem::Display => "Whole display".to_string(),
                    MenuItem::Monitor(monitor) => format!("Monitor {}", monitor.to_string()),
                    MenuItem::Window(window) => format!("Window {}", window.to_string()),
                }
            }
        }

        let mut items = Vec::from([MenuItem::Display]);
        items.extend(
            probe
                .windows
                .iter()
                .map(|window| MenuItem::Window(window.clone())),
        );
        items.extend(
            probe
                .monitors
                .iter()
                .map(|monitor| MenuItem::Monitor(monitor.clone())),
        );

        let selection = Select::new()
            .with_prompt("Which entity to capture?")
            .default(0)
            .items(&items)
            .interact()?;

        Ok(match &items[selection] {
            MenuItem::Display => "display".to_string(),
            MenuItem::Monitor(monitor) => format!("monitor{}", monitor.id),
            MenuItem::Window(window) => format!("window{}", window.id),
        })
    }

    pub fn qrexec_program_ident(&self) -> Result<String, Error> {
        let arg = self.region_selection()?;
        Ok(format!("{}+{arg}", IDENT))
    }
}

fn parse_u32_from_str(s: &str) -> Result<u32, String> {
    if let Some(s) = s.strip_prefix("0x") {
        u32::from_str_radix(s, 16)
    } else {
        s.parse()
    }
    .map_err(|err| err.to_string())
}

pub fn sigterm(pid: u32, verbose: bool) {
    let pid: i32 = pid.try_into().unwrap();
    if verbose {
        eprintln!("sending SIGTERM to pid {pid}");
    }
    unsafe {
        // Swallow errors. If it fails, we tried our best.
        libc::kill(pid, libc::SIGTERM);
    }
}

pub fn sigkill(pid: u32, verbose: bool) {
    let pid: i32 = pid.try_into().unwrap();
    if verbose {
        eprintln!("sending SIGKILL to pid {pid}");
    }
    unsafe {
        // Swallow errors. If it fails, we tried our best.
        libc::kill(pid, libc::SIGKILL);
    }
}
