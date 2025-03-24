use std::{
    env, fs, io,
    os::{
        fd::AsFd,
        unix::net::{UnixListener, UnixStream},
    },
    process::{Child, Command, Stdio},
    thread::{self},
};

use anyhow::Error;

use crate::{ConnectArgs, sigterm};

pub fn run(verbose: bool, args: ConnectArgs) -> Result<(), Error> {
    let viewer_socket = env::temp_dir().join("qyoobs-vnc.sock");
    let _ = fs::remove_file(viewer_socket.clone());
    let viewer_listener = UnixListener::bind(viewer_socket.clone())?;

    // TODO(inahga): I tried using pipes here, but it didn't work. FIFO buffering problem?
    let (mut qrexec_socket, qrexec_socket_child) = UnixStream::pair()?;
    let qrexec_handle = Command::new("qrexec-client-vm")
        .args([args.target_vmname(), &args.qrexec_program_ident()?])
        .stdin(qrexec_socket_child.as_fd().try_clone_to_owned()?)
        .stdout(qrexec_socket_child.as_fd().try_clone_to_owned()?)
        .stderr(if verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .spawn()?;

    drop(qrexec_socket_child);

    let mut viewer_handle = Command::new("vncviewer")
        .args([
            "-Shared",
            "-RemoteResize=0",
            "-SendPrimary=0",
            "-SendClipboard=0",
            if verbose { "-Log=*:stderr:100" } else { "" },
            viewer_socket
                .to_str()
                .expect("socket_path is not valid unicode?"),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(if verbose {
            Stdio::inherit()
        } else {
            Stdio::null()
        })
        .spawn()?;

    // This watchdog is primitive and can send double-sigterms, but that's no big deal.
    let _watchdog = Watchdog::new(qrexec_handle, viewer_handle.id(), verbose);

    let (mut viewer_stream, _) = viewer_listener.accept()?;
    {
        let mut stream = viewer_stream.try_clone()?;
        let mut qrexec_socket = qrexec_socket.try_clone()?;
        thread::spawn(move || io::copy(&mut qrexec_socket, &mut stream));
    }
    thread::spawn(move || io::copy(&mut viewer_stream, &mut qrexec_socket));

    viewer_handle.wait()?;
    Ok(())
}

struct Watchdog {
    qrexec_pid: u32,
    viewer_pid: u32,
    verbose: bool,
}

impl Watchdog {
    fn new(mut qrexec: Child, viewer_pid: u32, verbose: bool) -> Self {
        let qrexec_pid = qrexec.id();
        thread::spawn(move || {
            let result = qrexec.wait();
            match result {
                Ok(status) => eprintln!("qrexec-client-vm exited with {status}"),
                Err(err) => eprintln!("qrexec-client-vm exited: {err}"),
            }
            sigterm(viewer_pid, verbose);
        });

        Self {
            qrexec_pid,
            viewer_pid,
            verbose,
        }
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        sigterm(self.qrexec_pid, self.verbose);
        sigterm(self.viewer_pid, self.verbose);
    }
}
