// SPDX-FileCopyrightText: 2026 OpenXPlay.org
// SPDX-License-Identifier: AGPL-3.0-only
// Created by riko chen on 2026-10-15

use crate::Result;
use crate::*;
use carplay_airplay::*;
use carplay_iap2::{mfi::MfiAuthenticator, DuplexStream, Receive};
use std::{
    io::{Read, Write},
    net::{
        IpAddr, Ipv4Addr, Shutdown, SocketAddr, SocketAddrV6, TcpListener, TcpStream, UdpSocket,
    },
    os::fd::AsRawFd,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct SessionMedia {
    pub engine: Arc<CarPlayMediaEngine>,
}
pub struct TcpDuplex {
    socket: TcpStream,
    read: Mutex<()>,
    write: Mutex<()>,
    closed: AtomicBool,
}
impl TcpDuplex {
    pub fn new(socket: TcpStream) -> std::io::Result<Self> {
        socket.set_nodelay(true)?;
        Ok(Self {
            socket,
            read: Mutex::new(()),
            write: Mutex::new(()),
            closed: AtomicBool::new(false),
        })
    }
}
impl DuplexStream for TcpDuplex {
    fn send(&self, data: &[u8]) -> Result<()> {
        let _g = self.write.lock().unwrap();
        self.socket
            .set_write_timeout(Some(Duration::from_secs(10)))?;
        (&self.socket).write_all(data)
    }
    fn recv(&self, max: usize, timeout: Duration) -> Result<Receive> {
        let _g = self.read.lock().unwrap();
        if self.closed.load(Ordering::Acquire) {
            return Ok(Receive::Eof);
        }
        self.socket
            .set_read_timeout(Some(timeout.max(Duration::from_millis(1))))?;
        let mut b = vec![0; max];
        match (&self.socket).read(&mut b) {
            Ok(0) => Ok(Receive::Eof),
            Ok(n) => {
                b.truncate(n);
                Ok(Receive::Data(b))
            }
            // An expired read timeout is EAGAIN on Linux; ETIMEDOUT means keepalive gave up on the peer.
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(Receive::Timeout),
            Err(e) => Err(e),
        }
    }
    fn close(&self) -> Result<()> {
        if !self.closed.swap(true, Ordering::AcqRel) {
            let _ = self.socket.shutdown(Shutdown::Both);
        }
        Ok(())
    }
}
/// Detect a vanished phone on the control connection without treating an idle CarPlay screen as a
/// failure, as DiPlay's TcpLiveness does: probe after 10 s idle, every 3 s, give up after 3 probes,
/// and abandon unacknowledged data after 20 s.
fn configure_peer_liveness(socket: &TcpStream) -> std::io::Result<()> {
    #[cfg(not(target_os = "linux"))]
    {
        let _ = socket;
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "TCP keepalive tuning requires Linux",
        ))
    }
    #[cfg(target_os = "linux")]
    {
        for (level, name, value) in [
            (libc::SOL_SOCKET, libc::SO_KEEPALIVE, 1),
            (libc::IPPROTO_TCP, libc::TCP_KEEPIDLE, 10),
            (libc::IPPROTO_TCP, libc::TCP_KEEPINTVL, 3),
            (libc::IPPROTO_TCP, libc::TCP_KEEPCNT, 3),
            (libc::IPPROTO_TCP, libc::TCP_USER_TIMEOUT, 20_000),
        ] {
            let value: libc::c_int = value;
            let r = unsafe {
                libc::setsockopt(
                    socket.as_raw_fd(),
                    level,
                    name,
                    &value as *const _ as *const _,
                    std::mem::size_of_val(&value) as _,
                )
            };
            if r < 0 {
                return Err(std::io::Error::last_os_error());
            }
        }
        Ok(())
    }
}
/// Socket binding and routing scope for the iPhone-facing interface.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BindAddress {
    pub ip: IpAddr,
    pub scope_id: u32,
}
impl BindAddress {
    pub const fn new(ip: IpAddr, scope_id: u32) -> Self {
        Self { ip, scope_id }
    }
    pub fn socket_addr(self, port: u16) -> SocketAddr {
        match self.ip {
            IpAddr::V4(ip) => SocketAddr::new(ip.into(), port),
            IpAddr::V6(ip) => SocketAddr::V6(SocketAddrV6::new(ip, port, 0, self.scope_id)),
        }
    }
    fn scope_peer(self, peer: SocketAddr) -> SocketAddr {
        match peer {
            SocketAddr::V6(mut address) => {
                if address.scope_id() == 0 {
                    address.set_scope_id(self.scope_id);
                }
                SocketAddr::V6(address)
            }
            address => address,
        }
    }
}
impl From<IpAddr> for BindAddress {
    fn from(ip: IpAddr) -> Self {
        Self::new(ip, 0)
    }
}
impl From<Ipv4Addr> for BindAddress {
    fn from(ip: Ipv4Addr) -> Self {
        Self::new(ip.into(), 0)
    }
}

/// All local IPs, including the scoped IPv6 address used by the wired link.
fn host_ip_addresses() -> Result<Vec<IpAddr>> {
    let mut head = std::ptr::null_mut();
    if unsafe { libc::getifaddrs(&mut head) } != 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut ips = Vec::new();
    let mut row = head;
    while !row.is_null() {
        let address = unsafe { (*row).ifa_addr };
        if !address.is_null() {
            match unsafe { (*address).sa_family as i32 } {
                libc::AF_INET => ips.push(IpAddr::V4(Ipv4Addr::from(unsafe {
                    (*(address as *const libc::sockaddr_in))
                        .sin_addr
                        .s_addr
                        .to_ne_bytes()
                }))),
                libc::AF_INET6 => ips.push(IpAddr::V6(std::net::Ipv6Addr::from(unsafe {
                    (*(address as *const libc::sockaddr_in6)).sin6_addr.s6_addr
                }))),
                _ => {}
            }
        }
        row = unsafe { (*row).ifa_next };
    }
    unsafe { libc::freeifaddrs(head) };
    Ok(ips)
}
pub struct LinuxStreamTransport {
    bind_address: BindAddress,
    peer: Option<IpAddr>,
}
impl LinuxStreamTransport {
    pub fn new(bind_address: BindAddress) -> Self {
        Self {
            bind_address,
            peer: None,
        }
    }
    pub fn for_peer(bind_address: BindAddress, peer: IpAddr) -> Self {
        Self {
            bind_address,
            peer: Some(peer),
        }
    }
}
struct TcpAcceptor {
    listener: TcpListener,
    peer: IpAddr,
    closed: AtomicBool,
}
impl StreamListener for TcpAcceptor {
    fn port(&self) -> Result<u16> {
        Ok(self.listener.local_addr()?.port())
    }
    fn accept(&self, timeout: Duration) -> Result<Option<Arc<dyn DuplexStream>>> {
        let start = Instant::now();
        while !self.closed.load(Ordering::Acquire) {
            match self.listener.accept() {
                Ok((s, p)) => {
                    if self.peer.is_unspecified() || p.ip() == self.peer {
                        return Ok(Some(Arc::new(TcpDuplex::new(s)?)));
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e),
            }
            if start.elapsed() >= timeout {
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(None)
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}
struct UdpTransport {
    socket: UdpSocket,
    peer: SocketAddr,
    bind_address: BindAddress,
    read: Mutex<()>,
    closed: AtomicBool,
}
impl DatagramTransport for UdpTransport {
    fn set_receive_buffer_bytes(&self, bytes: usize) -> Result<()> {
        let n = bytes.min(i32::MAX as usize) as libc::c_int;
        let r = unsafe {
            libc::setsockopt(
                self.socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_RCVBUF,
                &n as *const _ as *const _,
                std::mem::size_of_val(&n) as _,
            )
        };
        if r < 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    fn local_port(&self) -> Result<u16> {
        Ok(self.socket.local_addr()?.port())
    }
    fn recv(&self, max: usize, t: Duration) -> Result<Receive> {
        if self.closed.load(Ordering::Acquire) {
            return Ok(Receive::Eof);
        }
        Ok(match self.recv_from(max, t)? {
            Some((b, _)) => Receive::Data(b),
            None => Receive::Timeout,
        })
    }
    fn send(&self, b: &[u8]) -> Result<()> {
        self.send_to(b, self.peer)
    }
    fn send_to(&self, b: &[u8], peer: SocketAddr) -> Result<()> {
        if self.closed.load(Ordering::Acquire) {
            return Err(std::io::Error::other("Datagram closed"));
        }
        if !self.peer.ip().is_unspecified() && peer.ip() != self.peer.ip() {
            return Err(std::io::Error::other("Unexpected datagram peer"));
        }
        self.socket.send_to(b, self.bind_address.scope_peer(peer))?;
        Ok(())
    }
    fn recv_from(&self, max: usize, t: Duration) -> Result<Option<(Vec<u8>, SocketAddr)>> {
        let _g = self.read.lock().unwrap();
        let start = Instant::now();
        while !self.closed.load(Ordering::Acquire) {
            let left = t.saturating_sub(start.elapsed());
            if left.is_zero() {
                break;
            }
            self.socket
                .set_read_timeout(Some(left.min(Duration::from_millis(100))))?;
            let mut b = vec![0; max];
            match self.socket.recv_from(&mut b) {
                Ok((n, p)) => {
                    if (self.peer.ip().is_unspecified() || p.ip() == self.peer.ip())
                        && (self.peer.port() == 0 || p.port() == self.peer.port())
                    {
                        b.truncate(n);
                        return Ok(Some((b, p)));
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(e) => return Err(e),
            }
        }
        Ok(None)
    }
    fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        Ok(())
    }
}
impl StreamTransport for LinuxStreamTransport {
    fn listen(&self, peer: IpAddr) -> Result<Arc<dyn StreamListener>> {
        let listener = TcpListener::bind(self.bind_address.socket_addr(0))?;
        listener.set_nonblocking(true)?;
        Ok(Arc::new(TcpAcceptor {
            listener,
            peer: self.peer.unwrap_or(peer),
            closed: AtomicBool::new(false),
        }))
    }
    fn bind_datagram(&self, peer: SocketAddr) -> Result<Arc<dyn DatagramTransport>> {
        Ok(Arc::new(UdpTransport {
            socket: UdpSocket::bind(self.bind_address.socket_addr(0))?,
            peer: self
                .bind_address
                .scope_peer(SocketAddr::new(self.peer.unwrap_or(peer.ip()), peer.port())),
            bind_address: self.bind_address,
            read: Mutex::new(()),
            closed: AtomicBool::new(false),
        }))
    }
}
type MediaFactory = Arc<
    dyn Fn(Arc<dyn SessionControl>, Arc<dyn StreamTransport>) -> Arc<CarPlayMediaEngine>
        + Send
        + Sync,
>;
struct ServerState {
    config: AirPlayConfig,
    sessions: Vec<Arc<AirPlaySession>>,
    socket: Option<TcpListener>,
}
pub struct LinuxAirPlayServer {
    address: BindAddress,
    state: Arc<Mutex<ServerState>>,
    identity: Arc<AirPlayIdentity>,
    pairings: Arc<dyn PairingStore>,
    mfi: Arc<dyn MfiAuthenticator>,
    media: Arc<dyn AirPlayMediaHandler>,
    listener: Arc<dyn AirPlaySessionListener>,
    transport: Arc<dyn StreamTransport>,
    closed: Arc<AtomicBool>,
    worker: Mutex<Option<std::thread::JoinHandle<()>>>,
    factory: Option<MediaFactory>,
    listener_identity: AirPlayListenerIdentity,
}
impl LinuxAirPlayServer {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        address: BindAddress,
        config: AirPlayConfig,
        identity: Arc<AirPlayIdentity>,
        pairings: Arc<dyn PairingStore>,
        mfi: Arc<dyn MfiAuthenticator>,
        media: Arc<dyn AirPlayMediaHandler>,
        listener: Arc<dyn AirPlaySessionListener>,
        transport: Arc<dyn StreamTransport>,
    ) -> Self {
        Self {
            address,
            state: Arc::new(Mutex::new(ServerState {
                config,
                sessions: Vec::new(),
                socket: None,
            })),
            identity,
            pairings,
            mfi,
            media,
            listener,
            transport,
            closed: Arc::new(AtomicBool::new(false)),
            worker: Mutex::new(None),
            factory: None,
            listener_identity: AirPlayListenerIdentity::new(0),
        }
    }
    pub fn listener_identity(&self) -> AirPlayListenerIdentity {
        self.listener_identity.clone()
    }
    pub fn with_media_factory(mut self, factory: MediaFactory) -> Self {
        self.factory = Some(factory);
        self
    }
    pub fn start(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap();
        if state.socket.is_some() || self.closed.load(Ordering::Acquire) {
            return Err(std::io::Error::other(
                "AirPlay server already started or closed",
            ));
        }
        let selected = Mutex::new(None);
        AirPlayPortSelector::select(state.config.port, &|port| {
            let socket = TcpListener::bind(self.address.socket_addr(port))?;
            let port = socket.local_addr()?.port();
            *selected.lock().unwrap() = Some(socket);
            Ok(port)
        })?;
        let socket = selected.into_inner().unwrap().unwrap();
        state.config.port = socket.local_addr()?.port();
        socket.set_nonblocking(true)?;
        state.socket = Some(socket.try_clone()?);
        drop(state);
        let (state, identity, pairings, mfi, media, listener, closed, factory, base_transport) = (
            self.state.clone(),
            self.identity.clone(),
            self.pairings.clone(),
            self.mfi.clone(),
            self.media.clone(),
            self.listener.clone(),
            self.closed.clone(),
            self.factory.clone(),
            self.transport.clone(),
        );
        let listener_identity = self.listener_identity.clone();
        *self.worker.lock().unwrap() = Some(crate::platform::spawn_worker(
            "airplay-accept",
            Box::new(move || {
                while !closed.load(Ordering::Acquire) {
                    match socket.accept() {
                        Ok((tcp, peer)) => {
                            // Local probes must not replace the phone's active session.
                            if peer.ip().is_loopback()
                                || host_ip_addresses().unwrap_or_default().contains(&peer.ip())
                            {
                                let _ = tcp.shutdown(Shutdown::Both);
                                continue;
                            }
                            listener.on_tcp_accepted(AirPlayTcpAccepted {
                                listener: listener_identity.clone(),
                                internal: false,
                                accepted_at_nanos: oxp_core::monotonic_nanos() as u64,
                            });
                            let liveness = configure_peer_liveness(&tcp);
                            let result = (|| -> Result<()> {
                                let control =
                                    Arc::new(DeferredControl(Mutex::new(std::sync::Weak::new())));
                                let transport: Arc<dyn StreamTransport> = Arc::new(PeerTransport {
                                    base: base_transport.clone(),
                                    peer: peer.ip(),
                                });
                                let engine = factory
                                    .as_ref()
                                    .map(|f| f(control.clone(), transport.clone()));
                                let handler: Arc<dyn AirPlayMediaHandler> = engine
                                    .as_ref()
                                    .map(|e| e.clone() as Arc<dyn AirPlayMediaHandler>)
                                    .unwrap_or_else(|| media.clone());
                                let session = AirPlaySession::new(
                                    Arc::new(TcpDuplex::new(tcp)?),
                                    state.lock().unwrap().config.clone(),
                                    identity.clone(),
                                    pairings.clone(),
                                    mfi.clone(),
                                    handler,
                                    listener.clone(),
                                )?;
                                session.log_debug(&match &liveness {
                                    Ok(()) => "TCP peer health enabled idle=10s interval=3s count=3 deadline=20000ms; no video-idle timeout".to_string(),
                                    Err(e) => format!("TCP peer health tuning unavailable: {e}"),
                                });
                                *control.0.lock().unwrap() = Arc::downgrade(&session);
                                session.configure_transport(transport, peer.ip())?;
                                if let Some(e) = engine {
                                    e.bind_session(&session, peer.ip());
                                }
                                let mut s = state.lock().unwrap();
                                s.sessions.retain(|x| !x.is_closed());
                                s.sessions.push(session.clone());
                                drop(s);
                                session.start()
                            })();
                            if let Err(e) = result {
                                listener.on_transport_error(&e.to_string());
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(20))
                        }
                        Err(e) => {
                            if !closed.load(Ordering::Acquire) {
                                listener.on_transport_error(&e.to_string());
                            }
                            break;
                        }
                    }
                }
            }),
        )?);
        Ok(())
    }
    pub fn config(&self) -> AirPlayConfig {
        self.state.lock().unwrap().config.clone()
    }
    pub fn close(&self) -> Result<()> {
        self.closed.store(true, Ordering::Release);
        let sessions = {
            let mut s = self.state.lock().unwrap();
            s.socket = None;
            std::mem::take(&mut s.sessions)
        };
        for s in sessions {
            let _ = s.close();
        }
        if let Some(w) = self.worker.lock().unwrap().take() {
            let _ = w.join();
        }
        Ok(())
    }
}
struct DeferredControl(Mutex<std::sync::Weak<AirPlaySession>>);
impl SessionControl for DeferredControl {
    fn playback_token(&self) -> Option<PlaybackToken> {
        self.0
            .lock()
            .unwrap()
            .upgrade()
            .and_then(|s| s.playback_token())
    }
    fn report_video_frame_rendered(&self, token: &PlaybackToken) -> bool {
        self.0
            .lock()
            .unwrap()
            .upgrade()
            .is_some_and(|s| s.report_video_frame_rendered(token))
    }
    fn send_media_button(&self, index: u8) -> Result<bool> {
        match self.0.lock().unwrap().upgrade() {
            Some(s) => s.send_media_button(index),
            None => Ok(false),
        }
    }
    fn show_view_area(&self, index: usize) -> bool {
        self.0
            .lock()
            .unwrap()
            .upgrade()
            .is_some_and(|s| s.show_view_area(index))
    }
    fn send_touch(&self, c: &[AirPlayContact]) -> Result<bool> {
        match self.0.lock().unwrap().upgrade() {
            Some(s) => s.send_touch(c),
            None => Ok(false),
        }
    }
}

/// Even wildcard media listeners are restricted to the accepted control connection's peer.
struct PeerTransport {
    base: Arc<dyn StreamTransport>,
    peer: IpAddr,
}
impl StreamTransport for PeerTransport {
    fn listen(&self, _peer: IpAddr) -> Result<Arc<dyn StreamListener>> {
        self.base.listen(self.peer)
    }
    fn bind_datagram(&self, peer: SocketAddr) -> Result<Arc<dyn DatagramTransport>> {
        self.base
            .bind_datagram(SocketAddr::new(self.peer, peer.port()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ipv4_and_ipv6_listen_accept_and_datagram() {
        for ip in ["127.0.0.1", "::1"].map(|s| s.parse::<IpAddr>().unwrap()) {
            let transport = LinuxStreamTransport::new(BindAddress::new(ip, 0));
            let listener = transport.listen(ip).unwrap();
            let mut client =
                TcpStream::connect(SocketAddr::new(ip, listener.port().unwrap())).unwrap();
            client.write_all(b"stream").unwrap();
            let accepted = listener.accept(Duration::from_secs(2)).unwrap().unwrap();
            assert_eq!(
                accepted.recv(10, Duration::from_secs(1)).unwrap(),
                Receive::Data(b"stream".to_vec())
            );
            accepted.close().unwrap();
            listener.close().unwrap();
            let peer = UdpSocket::bind(SocketAddr::new(ip, 0)).unwrap();
            peer.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
            let datagram = transport.bind_datagram(peer.local_addr().unwrap()).unwrap();
            peer.send_to(b"in", SocketAddr::new(ip, datagram.local_port().unwrap()))
                .unwrap();
            assert_eq!(
                datagram.recv(10, Duration::from_secs(1)).unwrap(),
                Receive::Data(b"in".to_vec())
            );
            datagram.send(b"out").unwrap();
            let mut bytes = [0; 10];
            assert_eq!(peer.recv_from(&mut bytes).unwrap().0, 3);
            assert_eq!(&bytes[..3], b"out");
            datagram.close().unwrap();
        }
    }
    #[test]
    fn link_local_scope_is_preserved_for_bind_and_peer_routing() {
        let address = BindAddress::new("fe80::2".parse().unwrap(), 42);
        assert_eq!(
            address.socket_addr(7000),
            "[fe80::2%42]:7000".parse().unwrap()
        );
        assert_eq!(
            address.scope_peer("[fe80::1]:1234".parse().unwrap()),
            "[fe80::1%42]:1234".parse().unwrap()
        );
        assert_eq!(
            address.scope_peer("[fe80::1%7]:1234".parse().unwrap()),
            "[fe80::1%7]:1234".parse().unwrap()
        );
        assert_eq!(
            BindAddress::from(Ipv4Addr::LOCALHOST).socket_addr(7000),
            "127.0.0.1:7000".parse().unwrap()
        );
    }
    #[test]
    fn own_addresses_include_ipv6_loopback() {
        let addresses = host_ip_addresses().unwrap();
        assert!(addresses.contains(&"127.0.0.1".parse().unwrap()));
        assert!(addresses.contains(&"::1".parse().unwrap()));
    }
}
