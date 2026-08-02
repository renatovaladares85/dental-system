use std::net::{SocketAddr, TcpListener};

use socket2::{Domain, Protocol, Socket, Type};

#[cfg(windows)]
fn enable_exclusive_address_use(socket: &Socket) -> std::io::Result<()> {
    use std::os::windows::io::AsRawSocket;
    use windows_sys::Win32::Networking::WinSock::{
        SO_EXCLUSIVEADDRUSE, SOCKET_ERROR, SOL_SOCKET, setsockopt,
    };

    let enabled: i32 = 1;
    // Windows permits overlapping binds unless exclusivity is requested. Set it
    // before bind so a LAN listener cannot silently share its service port.
    let result = unsafe {
        setsockopt(
            socket.as_raw_socket() as usize,
            SOL_SOCKET,
            SO_EXCLUSIVEADDRUSE,
            (&enabled as *const i32).cast(),
            std::mem::size_of::<i32>() as i32,
        )
    };
    if result == SOCKET_ERROR {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn bind_dual_stack_listener(port: u16) -> std::io::Result<TcpListener> {
    let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
    socket.set_only_v6(false)?;
    if socket.only_v6()? {
        return Err(std::io::Error::other(
            "dual-stack listener could not be enabled",
        ));
    }
    #[cfg(windows)]
    enable_exclusive_address_use(&socket)?;
    socket.bind(&SocketAddr::from(([0_u16; 8], port)).into())?;
    socket.listen(128)?;
    Ok(socket.into())
}
