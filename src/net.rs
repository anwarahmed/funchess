//! Two computers, one game: a direct TCP connection carrying one short line of text
//! per event. One player hosts (listens), the other joins (connects). Each side checks
//! the other's moves against the rules itself, so neither has to trust the other.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Shutdown, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::Duration;

use crate::chess::{Color, Move};

pub const DEFAULT_PORT: u16 = 6464;
/// The first line each side sends. The number changes whenever the messages do, so
/// that two versions that cannot understand each other say so instead of misbehaving.
const HELLO: &str = "funchess 1";
/// No message is anywhere near this long; a longer line is not from funchess.
const MAX_LINE: u64 = 256;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Message {
    Move(Move),
    Resign,
    DrawOffer,
    DrawAccept,
    DrawDecline,
    /// "I would like to play again." A new game starts once both sides have said so.
    Rematch,
}

impl Message {
    fn encode(self) -> String {
        match self {
            Message::Move(m) => format!("move {}", m.uci()),
            Message::Resign => "resign".into(),
            Message::DrawOffer => "draw offer".into(),
            Message::DrawAccept => "draw accept".into(),
            Message::DrawDecline => "draw decline".into(),
            Message::Rematch => "rematch".into(),
        }
    }

    fn parse(line: &str) -> Option<Message> {
        Some(match line {
            "resign" => Message::Resign,
            "draw offer" => Message::DrawOffer,
            "draw accept" => Message::DrawAccept,
            "draw decline" => Message::DrawDecline,
            "rematch" => Message::Rematch,
            _ => Message::Move(Move::parse_uci(line.strip_prefix("move ")?)?),
        })
    }
}

/// A connection that is still being made: a host waiting for someone to join, or a
/// joiner waiting for the host to answer. Dropping it gives up.
pub struct Pending {
    rx: Receiver<Result<TcpStream, String>>,
    stop: Arc<AtomicBool>,
}

impl Pending {
    /// `None` while still waiting.
    pub fn poll(&self) -> Option<Result<TcpStream, String>> {
        self.rx.try_recv().ok()
    }
}

impl Drop for Pending {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Starts listening for one opponent. Fails at once if the port cannot be used.
pub fn host(port: u16) -> io::Result<Pending> {
    // "::" takes IPv4 connections too where the system allows; otherwise IPv4 only.
    let listener = TcpListener::bind(("::", port)).or_else(|_| TcpListener::bind(("0.0.0.0", port)))?;
    listener.set_nonblocking(true)?;
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    thread::spawn(move || {
        while !stopped.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = tx.send(stream.set_nonblocking(false).map(|_| stream).map_err(|e| e.to_string()));
                    return;
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => thread::sleep(Duration::from_millis(50)),
                Err(e) => {
                    let _ = tx.send(Err(e.to_string()));
                    return;
                }
            }
        }
    });
    Ok(Pending { rx, stop })
}

/// Starts connecting to a host given as `name`, `name:port`, an IPv4 address or a
/// bracketed IPv6 address, with the default port when none is given.
pub fn join(address: &str) -> Pending {
    let (tx, rx) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    let address = address.trim().to_string();
    thread::spawn(move || {
        let _ = tx.send(connect(&address));
    });
    Pending { rx, stop }
}

fn connect(address: &str) -> Result<TcpStream, String> {
    let bare = address.trim_start_matches('[').trim_end_matches(']');
    let targets: Vec<_> = match bare.parse::<IpAddr>() {
        Ok(ip) => vec![(ip, DEFAULT_PORT).into()],
        Err(_) => address.to_socket_addrs().or_else(|_| (address, DEFAULT_PORT).to_socket_addrs()).map_err(|_| format!("cannot find \"{address}\""))?.collect(),
    };
    let mut error = format!("cannot find \"{address}\"");
    for target in targets {
        match TcpStream::connect_timeout(&target, Duration::from_secs(8)) {
            Ok(stream) => return Ok(stream),
            Err(e) => error = why_not(&target, &e),
        }
    }
    Err(error)
}

/// Why a connection failed, in terms of what to do about it. Silence nearly always
/// means a firewall on the host dropped the attempt; a refusal means the computer is
/// there and nothing is listening.
fn why_not(target: &std::net::SocketAddr, error: &io::Error) -> String {
    let (ip, port) = (target.ip(), target.port());
    match error.kind() {
        io::ErrorKind::TimedOut => format!("no answer from {ip} (is its firewall blocking port {port}?)"),
        io::ErrorKind::ConnectionRefused => format!("{ip} is there, but nobody is hosting on port {port}"),
        _ => format!("cannot reach {target}: {error}"),
    }
}

/// The command that lets other computers in on `port`, when this computer runs the
/// ufw firewall and has no rule for the port yet. Without it a joiner gets no answer.
pub fn firewall_command(port: u16) -> Option<String> {
    ufw_command(port, &std::fs::read_to_string("/etc/ufw/ufw.conf").ok()?, &std::fs::read_to_string("/etc/ufw/user.rules").unwrap_or_default())
}

fn ufw_command(port: u16, conf: &str, rules: &str) -> Option<String> {
    let on = conf.lines().any(|l| l.trim().eq_ignore_ascii_case("ENABLED=yes"));
    let open =
        rules.lines().any(|l| l.starts_with("-A ufw-user-input") && l.contains("ACCEPT") && l.contains(&format!("--dport {port} ")) && !l.contains("-p udp"));
    (on && !open).then(|| format!("sudo ufw allow {port}/tcp"))
}

/// This computer's address on the local network, for the host to tell the joiner.
/// Found by asking the system which address it would use to reach the internet;
/// "connecting" a UDP socket picks the route without sending anything.
pub fn local_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    socket.local_addr().ok().map(|a| a.ip()).filter(|ip| !ip.is_unspecified())
}

#[derive(Debug, PartialEq)]
pub enum Incoming {
    /// The other side is funchess and speaks the same version. From the host this
    /// carries the color the host plays.
    Hello(Option<Color>),
    Message(Message),
    /// The connection is over, with the reason.
    Closed(String),
}

/// An open connection to the opponent.
pub struct Link {
    stream: TcpStream,
    rx: Receiver<Incoming>,
}

impl Link {
    /// Greets the other side and starts listening. The host passes the color it plays.
    pub fn open(stream: TcpStream, host_color: Option<Color>) -> io::Result<Link> {
        stream.set_nodelay(true)?;
        let hello = match host_color {
            Some(Color::White) => "white",
            Some(Color::Black) => "black",
            None => "join",
        };
        let mut writer = stream.try_clone()?;
        writer.write_all(format!("{HELLO} {hello}\n").as_bytes())?;
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut reader = BufReader::new(stream);
            let mut greeted = false;
            loop {
                let incoming = match read_line(&mut reader) {
                    Err(reason) => Incoming::Closed(reason),
                    Ok(line) if !greeted => {
                        greeted = true;
                        match line.strip_prefix(HELLO).map(str::trim) {
                            Some("white") => Incoming::Hello(Some(Color::White)),
                            Some("black") => Incoming::Hello(Some(Color::Black)),
                            Some("join") => Incoming::Hello(None),
                            _ if line.starts_with("funchess ") => Incoming::Closed("the other player has a different version of funchess".into()),
                            _ => Incoming::Closed("the other end is not funchess".into()),
                        }
                    }
                    // A line that means nothing is skipped, not treated as an error.
                    Ok(line) => match Message::parse(&line) {
                        Some(message) => Incoming::Message(message),
                        None => continue,
                    },
                };
                let closed = matches!(incoming, Incoming::Closed(_));
                if tx.send(incoming).is_err() || closed {
                    return;
                }
            }
        });
        Ok(Link { stream: writer, rx })
    }

    /// A failure to send shows up as `Incoming::Closed` on the next `poll`.
    pub fn send(&mut self, message: Message) {
        if self.stream.write_all(format!("{}\n", message.encode()).as_bytes()).is_err() {
            let _ = self.stream.shutdown(Shutdown::Both);
        }
    }

    pub fn poll(&self) -> Option<Incoming> {
        self.rx.try_recv().ok()
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        // Ends the reading thread, and tells the other side we are gone.
        let _ = self.stream.shutdown(Shutdown::Both);
    }
}

fn read_line(reader: &mut BufReader<TcpStream>) -> Result<String, String> {
    let mut line = String::new();
    match reader.by_ref().take(MAX_LINE).read_line(&mut line) {
        Ok(0) => Err("the other player disconnected".into()),
        Ok(_) if line.ends_with('\n') => Ok(line.trim().to_string()),
        Ok(_) => Err("the other end is not funchess".into()),
        Err(e) if e.kind() == io::ErrorKind::InvalidData => Err("the other end is not funchess".into()),
        Err(_) => Err("the connection was lost".into()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn failures_say_what_to_do_and_a_closed_firewall_is_noticed() {
        let target = "192.168.1.20:6464".parse().unwrap();
        assert_eq!(super::why_not(&target, &io::ErrorKind::TimedOut.into()), "no answer from 192.168.1.20 (is its firewall blocking port 6464?)");
        assert_eq!(super::why_not(&target, &io::ErrorKind::ConnectionRefused.into()), "192.168.1.20 is there, but nobody is hosting on port 6464");

        let rules = "-A ufw-user-input -p udp --dport 6464 -j ACCEPT\n-A ufw-user-input -p tcp --dport 53317 -j ACCEPT\n";
        assert_eq!(super::ufw_command(6464, "ENABLED=yes\n", rules), Some("sudo ufw allow 6464/tcp".into()));
        assert_eq!(super::ufw_command(53317, "ENABLED=yes\n", rules), None);
        assert_eq!(super::ufw_command(6464, "ENABLED=yes\n", "-A ufw-user-input -p tcp --dport 6464 -j ACCEPT\n"), None);
        assert_eq!(super::ufw_command(6464, "ENABLED=no\n", rules), None);
    }

    use super::*;
    use std::time::Instant;

    fn wait<T>(mut poll: impl FnMut() -> Option<T>) -> T {
        let started = Instant::now();
        loop {
            if let Some(value) = poll() {
                return value;
            }
            assert!(started.elapsed() < Duration::from_secs(5), "timed out");
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// A host and a joiner on this machine, already past the greeting.
    fn pair(port: u16) -> (Link, Link) {
        let hosting = host(port).unwrap();
        let joining = join(&format!("127.0.0.1:{port}"));
        let mut a = Link::open(wait(|| hosting.poll()).unwrap(), Some(Color::Black)).unwrap();
        let b = Link::open(wait(|| joining.poll()).unwrap(), None).unwrap();
        assert_eq!(wait(|| a.poll()), Incoming::Hello(None));
        assert_eq!(wait(|| b.poll()), Incoming::Hello(Some(Color::Black)));
        a.send(Message::Rematch);
        assert_eq!(wait(|| b.poll()), Incoming::Message(Message::Rematch));
        (a, b)
    }

    #[test]
    fn messages_cross_in_both_directions() {
        let (mut a, mut b) = pair(46461);
        let mv = Move::parse_uci("e7e8q").unwrap();
        for message in [Message::Move(mv), Message::Resign, Message::DrawOffer, Message::DrawAccept, Message::DrawDecline] {
            a.send(message);
            assert_eq!(wait(|| b.poll()), Incoming::Message(message));
            b.send(message);
            assert_eq!(wait(|| a.poll()), Incoming::Message(message));
        }
    }

    #[test]
    fn leaving_is_noticed_by_the_other_side() {
        let (a, b) = pair(46462);
        drop(a);
        assert!(matches!(wait(|| b.poll()), Incoming::Closed(_)));
    }

    #[test]
    fn a_stranger_is_turned_away() {
        let hosting = host(46463).unwrap();
        let mut stranger = TcpStream::connect("127.0.0.1:46463").unwrap();
        let link = Link::open(wait(|| hosting.poll()).unwrap(), Some(Color::White)).unwrap();
        stranger.write_all(b"GET / HTTP/1.1\r\n").unwrap();
        assert_eq!(wait(|| link.poll()), Incoming::Closed("the other end is not funchess".into()));
    }

    #[test]
    fn a_wrong_address_is_reported() {
        assert!(wait(|| join("127.0.0.1:1").poll_blocking()).is_err());
    }

    impl Pending {
        fn poll_blocking(&self) -> Option<Result<TcpStream, String>> {
            self.rx.recv_timeout(Duration::from_secs(9)).ok()
        }
    }
}
