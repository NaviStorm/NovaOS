"""Parcours négatifs R2 distinct du témoin temporel déjà qualifié."""
from pathlib import Path
import runpy


def install(root):
    root=Path(root)
    p=root/'novaOS/user/novaauth/service/administration_bob_shell.rs'
    s=p.read_text()
    old='login.execute("admin-sonde", &["--r2-refus"])?'
    if s.count(old)!=1:raise ValueError('ancrage sonde sécurité R2 absent ou ambigu')
    p.write_text(s.replace(old,'login.execute("admin-sonde", &["--r2-securite"])?'))
    # Chemins de transport déjà qualifiés ; pas de modification des échéances.
    for name in ('administration_tls_trace.py','administration_transport_512_profile.py'):
        runpy.run_path(str(root/'novaOS/user/novaauth/service'/name))['install'](root)
