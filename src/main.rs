use anyhow::{Error, anyhow};
use argh::FromArgs;

mod connect;
mod server;

fn main() -> Result<(), Error> {
    let args: Args = argh::from_env();
    match args.subcommand {
        Subcommand::Server(sub) => server::run(sub.args),
        Subcommand::Connect(sub) => connect::run(args.verbose.unwrap_or_default(), sub),
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
}

/// Run the VNC server. You shouldn't normally need to run this yourself.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "server")]
pub struct ServerArgs {
    #[argh(positional)]
    args: Option<String>,
}

/// Open a VNC viewer to the target Qube.
#[derive(FromArgs, PartialEq, Debug)]
#[argh(subcommand, name = "connect")]
pub struct ConnectArgs {
    /// name of the target VM whose screen will be captured
    #[argh(positional)]
    target_vmname: String,

    /// id of the screen to capture
    ///
    /// You can identify the target screen's ID using `xrandr`.
    #[argh(option, short = 's', from_str_fn(parse_u32_from_str))]
    screen: Option<u32>,

    /// id of the window to capture.
    ///
    /// You can identify the target window's ID using `xwininfo -root -children`.
    #[argh(option, short = 'w', from_str_fn(parse_u32_from_str))]
    window: Option<u32>,
}

impl ConnectArgs {
    pub fn target_vmname(&self) -> &str {
        &self.target_vmname
    }

    fn region_selection(&self) -> Result<Option<String>, Error> {
        match (self.screen, self.window) {
            (None, None) => Ok(None),
            (None, Some(window)) => Ok(Some(format!("window0x{window:x}"))),
            (Some(screen), None) => Ok(Some(format!("screen{screen}"))),
            (Some(_), Some(_)) => Err(anyhow!("--screen is mutually exclusive with --window")),
        }
    }

    pub fn qrexec_program_ident(&self) -> Result<String, Error> {
        let ident = "qyoobs.VNC".to_string();
        Ok(match self.region_selection()? {
            Some(arg) => format!("{ident}+{arg}"),
            None => ident,
        })
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
