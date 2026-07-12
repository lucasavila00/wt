use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::thread;

fn main() {
    if let Err(error) = proxy() {
        eprintln!("wt: proxy devcontainer SSH: {error}");
        std::process::exit(1);
    }
}

fn proxy() -> Result<(), String> {
    let target = wt_guest::app_target()?;
    let address = format!("{}:{}", target.address, wt_guest::APP_SSH_PORT);
    let mut incoming = TcpStream::connect(&address)
        .map_err(|error| format!("connect to app SSH at {address}: {error}"))?;
    let mut outgoing = incoming
        .try_clone()
        .map_err(|error| format!("clone app SSH socket: {error}"))?;
    let input = thread::spawn(move || -> io::Result<()> {
        io::copy(&mut io::stdin().lock(), &mut outgoing)?;
        outgoing.shutdown(Shutdown::Write)
    });
    let mut stdout = io::stdout().lock();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = incoming
            .read(&mut buffer)
            .map_err(|error| format!("read app SSH stream: {error}"))?;
        if read == 0 {
            break;
        }
        stdout
            .write_all(&buffer[..read])
            .and_then(|()| stdout.flush())
            .map_err(|error| format!("write app SSH stream: {error}"))?;
    }
    input
        .join()
        .map_err(|_| "write app SSH stream thread panicked".to_owned())?
        .map_err(|error| format!("write app SSH stream: {error}"))
}
