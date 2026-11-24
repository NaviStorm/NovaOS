/// Le port privé vérifie la route avant cet appel. Lire la marque fixe et
/// contrôler la parenté sur le volume monté, sans ouvrir le fichier désigné.
pub(super) fn scope_seal(s:&mut Session<'_,'_>,q:&Frame)->rt::Result<Frame> {
    use novaos_identity::exec_file_scope::Probe;
    let probe=Probe::parse(q).map_err(|_|rt::BAD_ARGUMENT)?;
    s.mount_volume()?;
    let fs=s.fs.as_ref().ok_or(rt::CLOSED)?;
    let mut instance=[0;32];
    for (i,word) in instance.chunks_exact_mut(8).enumerate() {
        word.copy_from_slice(&exec_files_lab::config(468+i).to_le_bytes());
    }
    let seal=s.exec_seal()?.ok_or(rt::REVOKED)?;
    probe.inspect(fs,instance,&seal).map_err(|fault| match fault {
        Fault::Storage(Error::NotFound) | Fault::Denied =>rt::REVOKED,
        Fault::Storage(e) =>error(e),
        _ =>rt::REFUSED,
    })
}
