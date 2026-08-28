use crate::DuplexStream;
use socket2::{Domain, SockAddr, Socket, Type};
use std::io::{Read, Write};
use std::net::Shutdown;

pub struct VsockStream(Socket);

impl VsockStream {
    pub fn connect(cid: u32, port: u32) -> std::io::Result<Self> {
        let socket = Socket::new(Domain::VSOCK, Type::STREAM, None)?;
        socket.connect(&SockAddr::vsock(cid, port))?;
        Ok(Self(socket))
    }
}

impl Read for VsockStream {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        (&self.0).read(buffer)
    }
}

impl Write for VsockStream {
    fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
        (&self.0).write(buffer)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        (&self.0).flush()
    }
}

impl DuplexStream for VsockStream {
    fn try_clone_stream(&self) -> std::io::Result<Self> {
        self.0.try_clone().map(Self)
    }

    fn shutdown_stream(&self, how: Shutdown) -> std::io::Result<()> {
        self.0.shutdown(how)
    }
}

pub struct VsockListener(Socket);

impl VsockListener {
    pub fn bind(cid: u32, port: u32) -> std::io::Result<Self> {
        let socket = Socket::new(Domain::VSOCK, Type::STREAM, None)?;
        socket.bind(&SockAddr::vsock(cid, port))?;
        socket.listen(128)?;
        Ok(Self(socket))
    }

    pub fn accept(&self) -> std::io::Result<(VsockStream, u32)> {
        let (socket, address) = self.0.accept()?;
        let (cid, _) = address.as_vsock_address().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "accepted socket has no vsock peer address",
            )
        })?;
        Ok((VsockStream(socket), cid))
    }
}
