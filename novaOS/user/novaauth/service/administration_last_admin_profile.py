"""S7 dernier administrateur : terminer le client Bob après sa fermeture acquittée.

La sortie normale rend ses endpoints ; le noyau conserve ses quotas. Aucun arrêt
forcé ni délégation à Alice. Ce profil ne teste pas la révocation de Bob en vol.
"""
from pathlib import Path


def install(root):
    p=Path(root)/'novaOS/user/novaauth/service/administration_bob_shell.rs';s=p.read_text()
    start=s.index('    loop {',s.index('    mark(b"NAR2 Bob fermeture acquittee")?;'))
    p.write_text(s[:start]+'    Ok(())\n}\n')
