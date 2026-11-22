"""S6 QEMU : tickets expirés/consommés avec les flux NXADMIN1 existants."""
from pathlib import Path
import runpy


def install(root):
    root=Path(root)
    for name in ('administration_tls_trace.py','administration_transport_512_profile.py','administration_status_profile.py'):
        runpy.run_path(str(root/'novaOS/user/novaauth/service'/name))['install'](root)
    p=root/'novaOS/user/novaauth/service/administration_bob_shell.rs';s=p.read_text()
    start=s.index('    // Même chemin');end=s.index('    login.close()?;',start)
    s=s[:start]+s[end:];p.write_text(s)
    p=root/'components/shell/session_auth_lab.rs';s=p.read_text()
    start=s.index('    pub(super) fn ticket_probe(');end=s.index('    pub(super) fn exercise_relay',start)
    part=s[start:end]
    old='let key = self.ticket(name, rest, 5000, if matches!(*mode,"recu"|"concurrent") {1}else{0}, 0)?;'
    if part.count(old)!=1:raise ValueError('ancrage rôle S6')
    part=part.replace(old,'let key = self.ticket(name, rest, 5000, 1, 0)?;')
    if part.count('self.drain(&[pid],None)?;')!=1:raise ValueError('drainage existant S6 absent')
    p.write_text(s[:start]+part+s[end:])
