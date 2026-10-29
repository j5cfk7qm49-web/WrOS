use crate::{MAX_FRAME, Request, Response, decode, encode};
use std::{
    io::{Read, Write},
    os::{fd::AsRawFd, unix::net::UnixStream},
    path::Path,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

pub struct Client(UnixStream);
impl Client {
    pub fn connect(path: &Path) -> Result<Self, &'static str> {
        if !path.is_absolute() {
            return Err("Invalid login socket");
        }
        let socket = UnixStream::connect(path).map_err(|_| "Login service unavailable")?;
        let mut peer: libc::ucred = unsafe { std::mem::zeroed() };
        let mut size = std::mem::size_of_val(&peer) as libc::socklen_t;
        // greetd owns this local socket. Refuse user-controlled lookalike servers.
        let result = unsafe {
            libc::getsockopt(
                socket.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_PEERCRED,
                (&mut peer as *mut libc::ucred).cast(),
                &mut size,
            )
        };
        if result != 0 || size as usize != std::mem::size_of_val(&peer) || peer.uid != 0 {
            return Err("Login service identity was not accepted");
        }
        Ok(Self(socket))
    }
    pub fn interrupter(&self) -> Result<UnixStream, &'static str> {
        self.0.try_clone().map_err(|_| "Login service unavailable")
    }
    pub fn exchange(&mut self, request: &Request) -> Result<Response, &'static str> {
        self.exchange_with_budget(request, Duration::from_secs(15))
    }
    fn exchange_with_budget(
        &mut self,
        request: &Request,
        budget: Duration,
    ) -> Result<Response, &'static str> {
        let deadline = Instant::now() + budget;
        let payload = encode(request)?;
        self.write(&(payload.len() as u32).to_ne_bytes(), deadline)?;
        self.write(&payload, deadline)?;
        let mut header = [0; 4];
        self.read(&mut header, deadline)?;
        let size = u32::from_ne_bytes(header) as usize;
        if size == 0 || size > MAX_FRAME {
            return Err("Invalid login frame size");
        }
        let mut bytes = Zeroizing::new(vec![0; size]);
        self.read(&mut bytes, deadline)?;
        decode(&bytes)
    }
    fn read(&mut self, mut bytes: &mut [u8], deadline: Instant) -> Result<(), &'static str> {
        while !bytes.is_empty() {
            self.0
                .set_read_timeout(Some(remaining(deadline)?))
                .map_err(|_| "Login service unavailable")?;
            let count = self
                .0
                .read(bytes)
                .map_err(|_| "Login service read failed or timed out")?;
            if count == 0 {
                return Err("Login service disconnected");
            }
            bytes = &mut bytes[count..];
        }
        Ok(())
    }
    fn write(&mut self, mut bytes: &[u8], deadline: Instant) -> Result<(), &'static str> {
        while !bytes.is_empty() {
            self.0
                .set_write_timeout(Some(remaining(deadline)?))
                .map_err(|_| "Login service unavailable")?;
            let count = self
                .0
                .write(bytes)
                .map_err(|_| "Login service write failed or timed out")?;
            if count == 0 {
                return Err("Login service disconnected");
            }
            bytes = &bytes[count..];
        }
        Ok(())
    }
}
fn remaining(deadline: Instant) -> Result<Duration, &'static str> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|d| !d.is_zero())
        .ok_or("Login service timed out")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    #[test]
    fn refuses_a_nonroot_socket_peer() {
        assert_ne!(
            unsafe { libc::geteuid() },
            0,
            "run authentication tests unprivileged"
        );
        let path = std::env::temp_dir().join(format!(
            "wros-peer-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let result = Client::connect(&path);
        drop(listener);
        std::fs::remove_file(path).unwrap();
        assert!(matches!(
            result,
            Err("Login service identity was not accepted")
        ));
    }
    fn reply(bytes: Vec<u8>) -> Result<Response, &'static str> {
        let (a, mut b) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            let mut header = [0; 4];
            b.read_exact(&mut header).unwrap();
            let mut body = vec![0; u32::from_ne_bytes(header) as usize];
            b.read_exact(&mut body).unwrap();
            b.write_all(&bytes).unwrap();
        });
        let result = Client(a).exchange(&Request::CancelSession);
        worker.join().unwrap();
        result
    }
    #[test]
    fn rejects_huge_zero_truncated_and_invalid_frames() {
        for bytes in [
            u32::MAX.to_ne_bytes().to_vec(),
            0u32.to_ne_bytes().to_vec(),
            vec![1, 2],
            [10u32.to_ne_bytes().as_slice(), b"{}"].concat(),
            [2u32.to_ne_bytes().as_slice(), b"xx"].concat(),
        ] {
            assert!(reply(bytes).is_err());
        }
    }
    #[test]
    fn real_socket_round_trip_and_eof() {
        let body = br#"{"type":"success"}"#;
        assert!(matches!(
            reply([(body.len() as u32).to_ne_bytes().as_slice(), body].concat()),
            Ok(Response::Success)
        ));
        assert!(reply(vec![]).is_err());
    }
    #[test]
    fn response_has_total_deadline() {
        let (a, _b) = UnixStream::pair().unwrap();
        let start = Instant::now();
        assert!(
            Client(a)
                .exchange_with_budget(&Request::CancelSession, Duration::from_millis(30))
                .is_err()
        );
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn shutdown_interrupts_a_pending_read() {
        let (a, _b) = UnixStream::pair().unwrap();
        let mut client = Client(a);
        let interrupt = client.interrupter().unwrap();
        let thread = thread::spawn(move || client.exchange(&Request::CancelSession));
        interrupt.shutdown(std::net::Shutdown::Both).unwrap();
        assert!(thread.join().unwrap().is_err());
    }
}
