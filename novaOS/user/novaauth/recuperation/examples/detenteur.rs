//! Adaptateur de laboratoire. Une invocation accède à un seul détenteur.
//! Stdin et stdout ne contiennent que métadonnées publiques et chiffrés.
use ed25519_dalek::SigningKey;
use novaos_recovery::{
    DeliveryContext, Error, Manifest, RecoveryContextV2, open_key_proof, open_request, private,
    seal_key_proof, seal_share,
};
use rand_core::{CryptoRng, RngCore};
use std::io::{Read, Write};
use zeroize::Zeroizing;

struct Entropy(std::fs::File);
impl RngCore for Entropy {
    fn next_u32(&mut self) -> u32 {
        let mut b = [0; 4];
        self.fill_bytes(&mut b);
        u32::from_le_bytes(b)
    }
    fn next_u64(&mut self) -> u64 {
        let mut b = [0; 8];
        self.fill_bytes(&mut b);
        u64::from_le_bytes(b)
    }
    fn fill_bytes(&mut self, b: &mut [u8]) {
        // Aucun repli en cas de perte de l'entropie de l'hôte de laboratoire.
        if self.0.read_exact(b).is_err() {
            std::process::exit(3);
        }
    }
    fn try_fill_bytes(&mut self, b: &mut [u8]) -> Result<(), rand_core::Error> {
        self.fill_bytes(b);
        Ok(())
    }
}
impl CryptoRng for Entropy {}
fn seed(path: &str) -> Result<Zeroizing<[u8; 32]>, Error> {
    let mut f = std::fs::File::open(path).map_err(|_| Error::Format)?;
    let mut bytes = Zeroizing::new([0; 32]);
    f.read_exact(&mut bytes[..]).map_err(|_| Error::Format)?;
    if f.read(&mut [0]).map_err(|_| Error::Format)? != 0 {
        return Err(Error::Format);
    }
    Ok(bytes)
}
fn run() -> Result<(), Error> {
    let a: Vec<_> = std::env::args().collect();
    if a.get(1).is_some_and(|v| {
        v == "seal-key-proof"
            || v == "open-key-proof"
            || v == "deliver-v2"
            || v == "check-request-v2"
    }) {
        return versioned(&a);
    }
    if a.len() != 4 {
        return Err(Error::Format);
    }
    let role = a[1].parse::<u8>().map_err(|_| Error::Format)?;
    let key = private(&seed(&a[2])?[..])?;
    let signing = SigningKey::from_bytes(&*seed(&a[3])?);
    let mut input = [0; 530];
    std::io::stdin()
        .read_exact(&mut input)
        .map_err(|_| Error::Format)?;
    if std::io::stdin().read(&mut [0]).map_err(|_| Error::Format)? != 0 {
        return Err(Error::Format);
    }
    let manifest = Manifest::parse(&input[..418])?;
    let context = DeliveryContext {
        recovery: input[418..450].try_into().unwrap(),
        manifest: manifest.digest(),
        epoch: manifest.epoch(),
        delivery: input[450..466].try_into().unwrap(),
        nonce: input[466..498].try_into().unwrap(),
    };
    let share = manifest.share(role, &key)?;
    let mut rng = Entropy(std::fs::File::open("/dev/urandom").map_err(|_| Error::Crypto)?);
    let envelope = seal_share(
        role,
        &context,
        &share,
        input[498..530].try_into().unwrap(),
        &signing,
        &mut rng,
    )?;
    std::io::stdout()
        .write_all(&envelope)
        .map_err(|_| Error::Format)?;
    Ok(())
}
fn main() {
    if run().is_err() {
        eprintln!("Remise refusée");
        std::process::exit(1);
    }
}

fn input<const N: usize>() -> Result<Zeroizing<[u8; N]>, Error> {
    let mut bytes = Zeroizing::new([0; N]);
    std::io::stdin()
        .read_exact(&mut bytes[..])
        .map_err(|_| Error::Format)?;
    if std::io::stdin().read(&mut [0]).map_err(|_| Error::Format)? != 0 {
        return Err(Error::Format);
    }
    Ok(bytes)
}
fn output(bytes: &[u8]) -> Result<(), Error> {
    std::io::stdout()
        .write_all(bytes)
        .map_err(|_| Error::Format)
}
fn versioned(args: &[String]) -> Result<(), Error> {
    match (args[1].as_str(), args.len()) {
        ("seal-key-proof", 2) => {
            let input = input::<96>()?;
            let mut rng = Entropy(std::fs::File::open("/dev/urandom").map_err(|_| Error::Crypto)?);
            let challenge = Zeroizing::new(input[64..].try_into().unwrap());
            let result = seal_key_proof(
                input[..32].try_into().unwrap(),
                input[32..64].try_into().unwrap(),
                &challenge,
                &mut rng,
            )?;
            output(&result)
        }
        ("open-key-proof", 3) => {
            let input = input::<112>()?;
            let secret = private(&seed(&args[2])?[..])?;
            output(&open_key_proof(
                &secret,
                input[..32].try_into().unwrap(),
                &input[32..],
            )?)
        }
        ("check-request-v2", 4) => {
            let role = args[2].parse::<u8>().map_err(|_| Error::Format)?;
            let input = input::<716>()?;
            let recovery = RecoveryContextV2::parse(&input[..376])?;
            let context = recovery.delivery(
                input[376..392].try_into().unwrap(),
                input[392..424].try_into().unwrap(),
            );
            let key = private(&seed(&args[3])?[..])?;
            let sender = ed25519_dalek::VerifyingKey::from_bytes(&recovery.possession())
                .map_err(|_| Error::Crypto)?;
            open_request(role, &context, &key, &sender, &input[424..])
        }
        ("deliver-v2", 6) => {
            let role = args[2].parse::<u8>().map_err(|_| Error::Format)?;
            let input = input::<1134>()?;
            let manifest = Manifest::parse(&input[..418])?;
            let recovery = RecoveryContextV2::parse(&input[418..794])?;
            if manifest.digest() != recovery.manifest() || manifest.epoch() != recovery.epoch() {
                return Err(Error::Scope);
            }
            let context = recovery.delivery(
                input[794..810].try_into().unwrap(),
                input[810..842].try_into().unwrap(),
            );
            let request_key = private(&seed(&args[5])?[..])?;
            let possession = ed25519_dalek::VerifyingKey::from_bytes(&recovery.possession())
                .map_err(|_| Error::Crypto)?;
            open_request(role, &context, &request_key, &possession, &input[842..])?;
            let key = private(&seed(&args[3])?[..])?;
            let signing = SigningKey::from_bytes(&*seed(&args[4])?);
            let share = manifest.share(role, &key)?;
            let mut rng = Entropy(std::fs::File::open("/dev/urandom").map_err(|_| Error::Crypto)?);
            let envelope = seal_share(
                role,
                &context,
                &share,
                &recovery.recipient(role)?,
                &signing,
                &mut rng,
            )?;
            output(&envelope)
        }
        _ => Err(Error::Format),
    }
}
