use crate::{common, runtime as rt};
use core::cell::Cell;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use embedded_io::{ErrorKind, ErrorType, Read, Write};
use embedded_tls::{SignatureScheme, TlsError, blocking::*};
use rand_core::{CryptoRng, RngCore};
use sha2::Digest;

pub struct Source;
impl RngCore for Source {
    fn fill_bytes(&mut self, dst: &mut [u8]) {
        for chunk in dst.chunks_mut(32) {
            let secret = common::random().unwrap_or_else(|_| rt::exit(rt::REFUSED));
            chunk.copy_from_slice(&secret[..chunk.len()]);
        }
    }
    fn next_u32(&mut self) -> u32 {
        let mut x = [0; 4];
        self.fill_bytes(&mut x);
        u32::from_le_bytes(x)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = [0; 8];
        self.fill_bytes(&mut x);
        u64::from_le_bytes(x)
    }
    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(dst);
        Ok(())
    }
}
impl CryptoRng for Source {}

pub struct Verifier {
    transcript: Option<[u8; 32]>,
    pub verified: bool,
}
impl TlsVerifier<Aes128GcmSha256> for Verifier {
    fn set_hostname_verification(&mut self, h: &str) -> Result<(), TlsError> {
        if h == "autorite" {
            Ok(())
        } else {
            Err(TlsError::InvalidCertificate)
        }
    }
    fn verify_certificate(
        &mut self,
        d: &sha2::Sha256,
        cert: CertificateRef,
    ) -> Result<(), TlsError> {
        if cert.entries.len() == 2
            && !matches!(cert.entries.get(1),Some(CertificateEntryRef::X509(ca)) if *ca==include_bytes!(env!("NOVA_FIXTURE_CA")).as_slice())
        {
            return Err(TlsError::InvalidCertificate);
        }
        if cert.entries.len() > 2 {
            return Err(TlsError::InvalidCertificate);
        }
        match cert.entries.first() {
            Some(CertificateEntryRef::X509(der))
                if *der == include_bytes!(env!("NOVA_AUTHORITY_CERT")) =>
            {
                self.transcript = Some(d.clone().finalize().into());
                Ok(())
            }
            _ => Err(TlsError::InvalidCertificate),
        }
    }
    fn verify_signature(&mut self, v: CertificateVerifyRef) -> Result<(), TlsError> {
        let mut msg = [0x20; 130];
        msg[64..98].copy_from_slice(b"TLS 1.3, server CertificateVerify\0");
        msg[98..].copy_from_slice(&self.transcript.take().ok_or(TlsError::InvalidCertificate)?);
        if v.signature_scheme != SignatureScheme::Ed25519 {
            return Err(TlsError::InvalidSignatureScheme);
        }
        let public = VerifyingKey::from_bytes(include_bytes!(env!("NOVA_AUTHORITY_PUB")))
            .map_err(|_| TlsError::InvalidCertificate)?;
        public
            .verify_strict(
                &msg,
                &Signature::from_slice(v.signature).map_err(|_| TlsError::InvalidSignature)?,
            )
            .map_err(|_| TlsError::InvalidSignature)?;
        self.verified = true;
        Ok(())
    }
}
pub struct Local<'a> {
    key: &'a SigningKey,
    signed: &'a Cell<bool>,
}
impl signature::Signer<[u8; 64]> for Local<'_> {
    fn try_sign(&self, msg: &[u8]) -> Result<[u8; 64], signature::Error> {
        if msg.len() != 130
            || msg[..64] != [32; 64]
            || &msg[64..98] != b"TLS 1.3, client CertificateVerify\0"
        {
            return Err(signature::Error::new());
        }
        let result = self.key.sign(msg).to_bytes();
        self.signed.set(true);
        Ok(result)
    }
}
pub struct Provider<'a> {
    key: &'a SigningKey,
    check: Verifier,
    signed: Cell<bool>,
}
impl CryptoProvider for Provider<'_> {
    type CipherSuite = Aes128GcmSha256;
    type Signature = [u8; 64];
    fn rng(&mut self) -> impl rand_core::CryptoRngCore {
        Source
    }
    fn verifier(&mut self) -> Result<&mut impl TlsVerifier<Self::CipherSuite>, TlsError> {
        Ok(&mut self.check)
    }
    fn signer(
        &mut self,
    ) -> Result<(impl signature::SignerMut<Self::Signature>, SignatureScheme), TlsError> {
        Ok((
            Local {
                key: self.key,
                signed: &self.signed,
            },
            SignatureScheme::Ed25519,
        ))
    }
    fn client_cert(&mut self) -> Option<Certificate<impl AsRef<[u8]>>> {
        Some(Certificate::X509(
            include_bytes!(env!("NOVA_GUEST_CERT")).as_slice(),
        ))
    }
}

struct Wire {
    port: u64,
    opened: bool,
    until: u64,
    buf: [u8; 96],
    pos: usize,
    end: usize,
}
impl ErrorType for Wire {
    type Error = ErrorKind;
}
#[path = "../../../../../../abi/instances.rs"]
mod instances;
#[path = "../../../../../../abi/service_process.rs"]
mod process;
fn clock() -> Result<u64, ErrorKind> {
    rt::value(rt::call(instances::SERVICE_PROCESS, process::NOW, 0, 0))
        .map_err(|_| ErrorKind::PermissionDenied)
}
fn guard(until: u64) -> Result<u64, ErrorKind> {
    let now = clock()?;
    until
        .checked_sub(now)
        .filter(|&n| n != 0)
        .ok_or(ErrorKind::TimedOut)
}
fn permissions() -> Result<(), ErrorKind> {
    let mut frame = [0u64; 16];
    frame[0] = 1;
    let address = frame.as_mut_ptr() as u64;
    let time = clock()?;
    for (op, a, b) in [
        (process::START, address, 128),
        (process::KILL, address, 128),
        (process::POLL, address, 128),
        (process::DRAIN, 0, 0),
        (process::NOW, address, 0),
        (process::NOW, 0, 128),
        (process::NOW, address, 128),
        (u64::MAX, address, 128),
    ] {
        if rt::value(rt::call(instances::SERVICE_PROCESS, op, a, b)) != Err(rt::DENIED) {
            return Err(ErrorKind::PermissionDenied);
        }
    }
    if clock()? < time {
        return Err(ErrorKind::InvalidData);
    }
    common::marker(b"NAUTH signataire NOW autorise gestion et arguments refuses 8/8");
    Ok(())
}
fn ipc_error(status: u64) -> ErrorKind {
    if status == rt::TIMED_OUT {
        ErrorKind::TimedOut
    } else {
        ErrorKind::BrokenPipe
    }
}
impl Wire {
    fn exchange_fragment(&self, op: u8, data: &[u8]) -> Result<[u8; 128], ErrorKind> {
        let mut frame = [0; 128];
        frame[0] = op;
        frame[1] = data.len() as u8;
        frame[32..32 + data.len()].copy_from_slice(data);
        let tag = u64::from_le_bytes(*b"NATUN001");
        let local = self
            .until
            .min(clock()?.checked_add(2000).ok_or(ErrorKind::Other)?);
        loop {
            let send_ms = guard(local)?;
            let sent = rt::send_for(self.port, tag, &frame, send_ms);
            let remaining = guard(local)?;
            sent.map_err(ipc_error)?;
            let received = rt::receive_for(self.port, remaining);
            guard(local)?;
            let msg = received.map_err(ipc_error)?;
            if msg.len != 128 || msg.kind != 1 || msg.tag != tag {
                return Err(ErrorKind::InvalidData);
            }
            let reply = msg.payload;
            match common::number(&reply) {
                rt::OK => return Ok(reply),
                rt::WOULD_BLOCK => (),
                _ => return Err(ErrorKind::BrokenPipe),
            }
        }
    }
}
impl Drop for Wire {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.buf.zeroize();
        if !self.opened {
            return;
        }
        let tag = u64::from_le_bytes(*b"NACLOSE1");
        if rt::send_for(self.port, tag, &[0; 128], 500).is_ok() {
            // Un résultat de lecture antérieure peut encore attendre sa réception.
            for _ in 0..2 {
                let Ok(msg) = rt::receive_for(self.port, 500) else {
                    break;
                };
                if msg.kind == 1
                    && msg.tag == tag
                    && msg.len == 128
                    && common::number(&msg.payload) == rt::OK
                {
                    common::marker(b"NAUTH transport ferme");
                    return;
                }
            }
        }
        common::marker(b"NAUTH fermeture non confirmee");
    }
}
impl Read for Wire {
    fn read(&mut self, d: &mut [u8]) -> Result<usize, Self::Error> {
        if d.is_empty() {
            return Ok(0);
        }
        if self.pos == self.end {
            let r = self.exchange_fragment(3, &[])?;
            self.end = usize::from(r[8]);
            self.pos = 0;
            if !(1..=96).contains(&self.end) {
                return Err(ErrorKind::InvalidData);
            }
            self.buf[..self.end].copy_from_slice(&r[32..32 + self.end]);
        }
        let n = d.len().min(self.end - self.pos);
        d[..n].copy_from_slice(&self.buf[self.pos..self.pos + n]);
        self.pos += n;
        Ok(n)
    }
}
impl Write for Wire {
    fn write(&mut self, d: &[u8]) -> Result<usize, Self::Error> {
        let n = d.len().min(96);
        if n != 0 {
            self.exchange_fragment(2, &d[..n])?;
        }
        Ok(n)
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
pub fn probe(key: &SigningKey, id: &[u8; 32]) -> Result<(), TlsError> {
    permissions().map_err(TlsError::Io)?;
    common::marker(b"NAUTH TLS avant");
    let start = clock().map_err(TlsError::Io)?;
    let until = start.checked_add(10_000).ok_or(TlsError::IoError)?;
    struct Measurement(u64, u64);
    impl Drop for Measurement {
        fn drop(&mut self) {
            if let Ok(end) = clock() {
                super::report_time(self.0, self.1, end);
                common::marker(b"NAUTH TLS apres");
            }
        }
    }
    let _measurement = Measurement(start, until);
    let mut net = Wire {
        opened: false,
        port: rt::config(9),
        until,
        buf: [0; 96],
        pos: 0,
        end: 0,
    };
    net.opened = true;
    net.exchange_fragment(1, &common::random().map_err(|_| TlsError::IoError)?[..16])
        .map_err(TlsError::Io)?;
    let mut r = zeroize::Zeroizing::new([0; 2070]);
    let mut w = zeroize::Zeroizing::new([0; 2070]);
    let mut tls = TlsConnection::new(net, &mut r[..], &mut w[..]);
    let mut p = Provider {
        key,
        check: Verifier {
            transcript: None,
            verified: false,
        },
        signed: Cell::new(false),
    };
    let cfg = TlsConfig::new()
        .with_server_name("autorite")
        .with_max_fragment_length(MaxFragmentLength::Bits11);
    common::marker(b"NAUTH TLS actif");
    tls.open(TlsContext::new(&cfg, &mut p))?;
    guard(until).map_err(TlsError::Io)?;
    if !p.check.verified || !p.signed.get() {
        return Err(TlsError::InvalidCertificate);
    }
    if tls.write(id)? != id.len() {
        return Err(TlsError::IoError);
    }
    tls.flush()?;
    let mut reply = [0; 32];
    let mut pos = 0;
    while pos < reply.len() {
        let n = tls.read(&mut reply[pos..])?;
        if n == 0 {
            return Err(TlsError::ConnectionClosed);
        }
        pos += n;
    }
    guard(until).map_err(TlsError::Io)?;
    if reply != *id {
        return Err(TlsError::InvalidApplicationData);
    }
    Ok(())
}
