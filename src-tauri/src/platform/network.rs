use std::net::{SocketAddr, TcpListener};

use socket2::{Domain, Protocol, Socket, Type};

pub(crate) fn bind_dual_stack_listener(port: u16) -> std::io::Result<TcpListener> {
    let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_only_v6(false)?;
    if socket.only_v6()? {
        return Err(std::io::Error::other(
            "dual-stack listener could not be enabled",
        ));
    }
    socket.bind(&SocketAddr::from(([0_u16; 8], port)).into())?;
    socket.listen(128)?;
    Ok(socket.into())
}
