//! A small passive-mode FTP client (RFC 959): enough to download files and list
//! directories of FTP-only repositories.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use super::transport::{percent_decode, DirEntry};
use crate::error::{Error, Result};

/// The largest download accepted (the same cap as unpacking archives).
const LIMIT: u64 = 4 << 30;

/// The parts of an `ftp://[user[:password]@]host[:port]/path` URL.
struct FtpUrl {
    host: String,
    port: u16,
    user: String,
    password: String,
    path: String,
}

fn parse_url(url: &str) -> Option<FtpUrl> {
    let rest = url.get(..6).filter(|s| s.eq_ignore_ascii_case("ftp://")).map(|_| &url[6..])?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    let (credentials, hostport) = match authority.rsplit_once('@') {
        Some((c, h)) => (Some(c), h),
        None => (None, authority),
    };
    let (user, password) = match credentials {
        Some(c) => match c.split_once(':') {
            Some((u, p)) => (percent_decode(u), percent_decode(p)),
            None => (percent_decode(c), String::new()),
        },
        None => ("anonymous".to_string(), "anonymous@".to_string()),
    };
    let (host, port) = match hostport.rsplit_once(':') {
        Some((h, p)) if !h.is_empty() && p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty() => (h.to_string(), p.parse().ok()?),
        _ => (hostport.to_string(), 21),
    };
    if host.is_empty() {
        return None;
    }
    Some(FtpUrl { host: host.trim_matches(['[', ']']).to_string(), port, user, password, path: percent_decode(path) })
}

fn network(url: &str, message: impl std::fmt::Display) -> Error {
    Error::Network { url: url.to_string(), message: message.to_string() }
}

fn connect(addrs: impl ToSocketAddrs, timeout: Duration, url: &str) -> Result<TcpStream> {
    let mut last = None;
    for addr in addrs.to_socket_addrs().map_err(|e| network(url, e))? {
        match TcpStream::connect_timeout(&addr, timeout) {
            Ok(stream) => {
                stream.set_read_timeout(Some(timeout)).map_err(|e| network(url, e))?;
                stream.set_write_timeout(Some(timeout)).map_err(|e| network(url, e))?;
                return Ok(stream);
            }
            Err(e) => last = Some(e),
        }
    }
    Err(network(url, last.map_or_else(|| "no address".to_string(), |e| e.to_string())))
}

/// A logged-in control connection.
struct Session {
    url: String,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
    peer: SocketAddr,
    timeout: Duration,
}

impl Session {
    fn open(url: &str, parts: &FtpUrl, timeout: Duration) -> Result<Session> {
        let stream = connect((parts.host.as_str(), parts.port), timeout, url)?;
        let peer = stream.peer_addr().map_err(|e| network(url, e))?;
        let writer = stream.try_clone().map_err(|e| network(url, e))?;
        let mut s = Session { url: url.to_string(), reader: BufReader::new(stream), writer, peer, timeout };
        s.expect(&[2])?;
        let (code, text) = s.command(&format!("USER {}", parts.user))?;
        if code / 100 == 3 {
            let (code, text) = s.command(&format!("PASS {}", parts.password))?;
            if code / 100 != 2 {
                return Err(network(url, format!("login refused: {code} {text}")));
            }
        } else if code / 100 != 2 {
            return Err(network(url, format!("login refused: {code} {text}")));
        }
        let (code, text) = s.command("TYPE I")?;
        if code / 100 != 2 {
            return Err(network(url, format!("TYPE I refused: {code} {text}")));
        }
        Ok(s)
    }

    /// One reply (multi-line replies are joined): its code and text.
    fn reply(&mut self) -> Result<(u32, String)> {
        let mut text = String::new();
        let mut first_code: Option<String> = None;
        loop {
            let mut line = String::new();
            if self.reader.read_line(&mut line).map_err(|e| network(&self.url, e))? == 0 {
                return Err(network(&self.url, "connection closed"));
            }
            let line = line.trim_end_matches(['\r', '\n']);
            text.push_str(line);
            text.push('\n');
            let code = line.get(..3).filter(|c| c.chars().all(|ch| ch.is_ascii_digit()));
            match (&first_code, code) {
                (None, Some(c)) if line.as_bytes().get(3) == Some(&b'-') => first_code = Some(c.to_string()),
                (None, Some(c)) => return Ok((c.parse().unwrap_or(0), text)),
                (Some(start), Some(c)) if c == start && line.as_bytes().get(3) != Some(&b'-') => return Ok((c.parse().unwrap_or(0), text)),
                _ => {}
            }
        }
    }

    fn expect(&mut self, classes: &[u32]) -> Result<(u32, String)> {
        let (code, text) = self.reply()?;
        if classes.contains(&(code / 100)) {
            Ok((code, text))
        } else if code == 550 || code == 450 {
            Err(Error::NotFound(self.url.clone()))
        } else {
            Err(network(&self.url, format!("{code} {}", text.trim())))
        }
    }

    fn command(&mut self, cmd: &str) -> Result<(u32, String)> {
        self.writer.write_all(format!("{cmd}\r\n").as_bytes()).map_err(|e| network(&self.url, e))?;
        self.reply()
    }

    /// Open a passive data connection (PASV, or EPSV when PASV is refused).
    fn data_connection(&mut self) -> Result<TcpStream> {
        let (code, text) = self.command("PASV")?;
        if code == 227 {
            let body = match text.find('(') {
                Some(p) => &text[p + 1..],
                None => text.get(4..).unwrap_or(""),
            };
            let numbers: Vec<u32> = body.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).take(6).collect();
            if let [.., hi, lo] = numbers[..] {
                if numbers.len() == 6 {
                    let port = u16::try_from(hi * 256 + lo).map_err(|_| network(&self.url, "bad PASV reply"))?;
                    // The address in the reply is often a private one; the control peer is reachable.
                    return connect(SocketAddr::new(self.peer.ip(), port), self.timeout, &self.url);
                }
            }
        }
        let (code, text) = self.command("EPSV")?;
        if code == 229 {
            let port = text.split('|').filter_map(|s| s.parse::<u16>().ok()).next_back();
            if let Some(port) = port {
                return connect(SocketAddr::new(self.peer.ip(), port), self.timeout, &self.url);
            }
        }
        Err(network(&self.url, format!("no passive data connection: {code} {}", text.trim())))
    }

    /// Run a transfer command and read its data connection to the end.
    fn transfer(&mut self, cmd: &str) -> Result<Vec<u8>> {
        let data = self.data_connection()?;
        let (code, text) = self.command(cmd)?;
        if code == 550 || code == 450 {
            return Err(Error::NotFound(self.url.clone()));
        }
        if code / 100 != 1 && code / 100 != 2 {
            return Err(network(&self.url, format!("{code} {}", text.trim())));
        }
        let mut out = Vec::new();
        data.take(LIMIT).read_to_end(&mut out).map_err(|e| network(&self.url, e))?;
        if code / 100 == 1 {
            self.expect(&[2])?;
        }
        Ok(out)
    }

    fn quit(mut self) {
        let _ = self.command("QUIT");
    }
}

/// Download one file.
pub(crate) fn get(url: &str, timeout: Duration) -> Result<Vec<u8>> {
    let parts = parse_url(url).ok_or_else(|| network(url, "not an ftp:// URL"))?;
    let mut session = Session::open(url, &parts, timeout)?;
    let data = session.transfer(&format!("RETR {}", parts.path))?;
    session.quit();
    Ok(data)
}

/// List a directory.
pub(crate) fn list(url: &str, timeout: Duration) -> Result<Vec<DirEntry>> {
    let parts = parse_url(url).ok_or_else(|| network(url, "not an ftp:// URL"))?;
    let mut session = Session::open(url, &parts, timeout)?;
    let (code, _) = session.command(&format!("CWD {}", parts.path))?;
    if code / 100 != 2 {
        return Err(Error::NotFound(url.to_string()));
    }
    let listing = session.transfer("LIST")?;
    session.quit();
    Ok(String::from_utf8_lossy(&listing).lines().filter_map(parse_list_line).collect())
}

/// Split off `n` whitespace-separated fields; the rest (after its leading spaces) follows.
fn fields(line: &str, n: usize) -> Option<(Vec<&str>, &str)> {
    let mut rest = line;
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        rest = rest.trim_start();
        let end = rest.find(char::is_whitespace)?;
        out.push(&rest[..end]);
        rest = &rest[end..];
    }
    Some((out, rest.trim_start()))
}

/// One line of a `LIST` reply, in the Unix (`drwxr-xr-x 2 u g 4096 Jan 1 2020 name`) or
/// MS-DOS (`01-01-20 12:00AM <DIR> name`) style.
fn parse_list_line(line: &str) -> Option<DirEntry> {
    let line = line.trim_end();
    let first = line.chars().next()?;
    let entry = if matches!(first, 'd' | '-' | 'l') {
        let (f, name) = fields(line, 8)?;
        let name = if first == 'l' { name.split(" -> ").next().unwrap_or(name) } else { name };
        DirEntry { name: name.to_string(), is_dir: first == 'd', size: f[4].parse().ok() }
    } else if first.is_ascii_digit() {
        let (f, name) = fields(line, 3)?;
        DirEntry { name: name.to_string(), is_dir: f[2] == "<DIR>", size: f[2].parse().ok() }
    } else {
        return None;
    };
    (!entry.name.is_empty() && entry.name != "." && entry.name != "..").then_some(entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn urls_and_listings() {
        let u = parse_url("ftp://user:p%40ss@ftp.example.org:2121/pub/sword/").unwrap();
        assert_eq!((u.host.as_str(), u.port, u.user.as_str(), u.password.as_str(), u.path.as_str()), ("ftp.example.org", 2121, "user", "p@ss", "/pub/sword/"));
        let u = parse_url("FTP://ftp.crosswire.org/pub/a b").unwrap();
        assert_eq!((u.port, u.user.as_str(), u.path.as_str()), (21, "anonymous", "/pub/a b"));
        assert!(parse_url("http://x/").is_none());

        let d = parse_list_line("drwxr-xr-x    2 ftp      ftp          4096 Mar 01  2020 my dir").unwrap();
        assert_eq!(d, DirEntry { name: "my dir".into(), is_dir: true, size: Some(4096) });
        let f = parse_list_line("-rw-r--r--    1 ftp      ftp         12345 Mar 01 12:00 ot.bzz").unwrap();
        assert_eq!(f, DirEntry { name: "ot.bzz".into(), is_dir: false, size: Some(12345) });
        let w = parse_list_line("01-01-20  12:00AM       <DIR>          images").unwrap();
        assert_eq!(w, DirEntry { name: "images".into(), is_dir: true, size: None });
        let w = parse_list_line("01-01-20  12:00AM              77 nt.vss").unwrap();
        assert_eq!(w, DirEntry { name: "nt.vss".into(), is_dir: false, size: Some(77) });
        assert!(parse_list_line("total 12").is_none());
    }

    /// A scripted FTP server for one session: replies to each command in turn.
    fn fake_server(files: Vec<(&'static str, &'static [u8])>, listing: &'static str) -> u16 {
        let control = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = control.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (stream, _) = control.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut out = stream;
            let data = TcpListener::bind("127.0.0.1:0").unwrap();
            let dport = data.local_addr().unwrap().port();
            out.write_all(b"220-Welcome\r\n220 ready\r\n").unwrap();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let line = line.trim_end().to_string();
                let (cmd, arg) = line.split_once(' ').unwrap_or((&line, ""));
                let send = |out: &mut TcpStream, s: &str| out.write_all(s.as_bytes()).unwrap();
                match cmd {
                    "USER" => send(&mut out, "331 password please\r\n"),
                    "PASS" => send(&mut out, "230 ok\r\n"),
                    "TYPE" => send(&mut out, "200 binary\r\n"),
                    "CWD" => send(&mut out, if arg == "/missing/" { "550 no\r\n" } else { "250 ok\r\n" }),
                    "PASV" => send(&mut out, &format!("227 Entering Passive Mode (10,0,0,1,{},{})\r\n", dport / 256, dport % 256)),
                    "RETR" | "LIST" => {
                        let body: Option<&[u8]> = if cmd == "LIST" { Some(listing.as_bytes()) } else { files.iter().find(|(n, _)| *n == arg).map(|(_, b)| *b) };
                        let (mut conn, _) = data.accept().unwrap();
                        match body {
                            Some(body) => {
                                send(&mut out, "150 opening\r\n");
                                conn.write_all(body).unwrap();
                                drop(conn);
                                send(&mut out, "226 done\r\n");
                            }
                            None => send(&mut out, "550 no such file\r\n"),
                        }
                    }
                    "QUIT" => {
                        send(&mut out, "221 bye\r\n");
                        return;
                    }
                    _ => send(&mut out, "502 not implemented\r\n"),
                }
            }
        });
        port
    }

    #[test]
    fn talks_to_a_server() {
        let t = Duration::from_secs(5);
        let port = fake_server(vec![("/pub/x.conf", b"[X]\nModDrv=RawText\n")], "");
        assert_eq!(get(&format!("ftp://127.0.0.1:{port}/pub/x.conf"), t).unwrap(), b"[X]\nModDrv=RawText\n");
        let port = fake_server(vec![], "");
        assert!(matches!(get(&format!("ftp://127.0.0.1:{port}/pub/none"), t), Err(Error::NotFound(_))));
        let port = fake_server(vec![], "drwxr-xr-x 2 a b 0 Jan 1 2020 sub\r\n-rw-r--r-- 1 a b 9 Jan 1 2020 nt\r\n");
        let names: Vec<String> = list(&format!("ftp://127.0.0.1:{port}/pub/"), t).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["sub", "nt"]);
        let port = fake_server(vec![], "");
        assert!(matches!(list(&format!("ftp://127.0.0.1:{port}/missing/"), t), Err(Error::NotFound(_))));
    }
}
