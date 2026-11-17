//! Dialogue privé du travailleur. La capacité et le handle ouvert doivent
//! appartenir à la même vue. Un principal fraîchement admis n'a aucun handle.
use super::{
    document,
    documentation::Language,
    options,
    private::{Private, tmp},
    template,
};
use alloc::vec::Vec;
use nova_cli::{protocol as fs, runtime as rt};
use sha2::{Digest, Sha256};

use super::exchange::{read, write};
// Format privé de recette : magie, seed de 32 octets, langue et TMPDIR
// terminés par NUL, puis argv terminé par NUL. Résultat : magie, longueur u32, LF.
pub fn run(handle: u64) -> Result<u64, u64> {
    let view = Private(rt::value(rt::call(tmp::CAP, 0, 0, 0))?);
    let request = read(&view, handle)?;
    let data = request.strip_prefix(b"MKTREQ01").ok_or(rt::BAD_ARGUMENT)?;
    let (seed, data) = data.split_at_checked(32).ok_or(rt::BAD_ARGUMENT)?;
    if !data.ends_with(b"\0") {
        return Err(rt::BAD_ARGUMENT);
    }
    let mut fields = data[..data.len() - 1].split(|&b| b == 0);
    let lang =
        Language::from_tag(core::str::from_utf8(fields.next().ok_or(rt::BAD_ARGUMENT)?).ok());
    let tmpdir = fields.next().ok_or(rt::BAD_ARGUMENT)?;
    let args: Vec<_> = fields.collect();
    if args.len() > 32 {
        return Err(rt::LIMIT);
    }
    let mut created = None;
    let (code, output) = match options::parse(&args, false) {
        Ok(options::Action::Create(o)) => match template::prepare(&o, tmpdir, b"tmp") {
            Err(e) => (1, [b"mktemp: ".as_slice(), &e, b"\n"].concat()),
            Ok(t) => {
                let pid = rt::value(rt::call(rt::PROCESS_ID, 0, 0, 0))?;
                let mut ctr = 0u64;
                let fill = |dest: &mut [u8]| {
                    let mut used = 0;
                    while used < dest.len() {
                        let digest = Sha256::new()
                            .chain_update(seed)
                            .chain_update(pid.to_le_bytes())
                            .chain_update(ctr.to_le_bytes())
                            .finalize();
                        ctr += 1;
                        for b in digest {
                            if b < 248 && used < dest.len() {
                                const ALPHABET: &[u8]=b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
                                dest[used] = ALPHABET[(b % 62) as usize];
                                used += 1;
                            }
                        }
                    }
                };
                match template::reserve(&t, fill, |n| view.claim(n, o.directory, o.dry)) {
                    Ok(Some(mut name)) => {
                        if !o.dry {
                            created = Some((name.clone(), o.directory));
                        }
                        name.push(b'\n');
                        (0, name)
                    }
                    Ok(None) | Err(_) if o.quiet => (1, Vec::new()),
                    Err(e) => (
                        1,
                        alloc::format!("mktemp: {}\n", nova_cli::console::reason(e)).into_bytes(),
                    ),
                    Ok(None) => (1, b"mktemp: no free name\n".to_vec()),
                }
            }
        },
        Ok(options::Action::Help) => (0, document(true, lang).into_bytes()),
        Ok(options::Action::Version) => (0, document(false, lang).into_bytes()),
        Err(e) => (
            1,
            [
                b"mktemp: ".as_slice(),
                &e,
                b"\nTry 'mktemp --help' for more information.\n",
            ]
            .concat(),
        ),
    };
    let response = [
        b"MKTRES01".as_slice(),
        &(output.len() as u32).to_le_bytes(),
        &output,
    ]
    .concat();
    if let Err(e) = write(&view, handle, &response) {
        if e != fs::UNCERTAIN
            && let Some((name, dir)) = created
        {
            view.named(
                if dir { fs::RMDIR } else { fs::UNLINK_AT },
                name.strip_prefix(b"tmp/").unwrap_or(&name),
                0,
            )?;
        }
        return Err(e);
    }
    Ok(code)
}
