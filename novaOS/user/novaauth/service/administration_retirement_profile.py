"""Effecteurs R2 approuvés, sur les conversations existantes du seul export QEMU."""
from pathlib import Path


def install(root):
    root=Path(root)
    def edit(name,old,new):
        p=root/name;s=p.read_text()
        if s.count(old)!=1:raise ValueError('ancrage retrait R2 absent ou ambigu : '+name+': '+old[:80])
        p.write_text(s.replace(old,new))
    verifier='novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    edit(verifier,"struct Login<'a> {", """struct Login<'a> {
    retirement_binding:Option<([u8;16],u64,[u8;32])>,
    kernel_retired:Option<[u8;32]>,""")
    edit(verifier,'            admin_close_step:0,','            admin_close_step:0,\n            retirement_binding:None, kernel_retired:None,')
    edit(verifier,'        self.live = Some((id, reference));', '''        self.live = Some((id, reference));
        self.retirement_binding=Some((current.subject,current.epoch,id));
        self.kernel_retired=None;''')
    edit(verifier,'                Ok(()) | Err(rt::CLOSED | rt::REVOKED) => (),',
         '''                Ok(()) => {self.kernel_retired=Some(id);},
                Err(rt::CLOSED | rt::REVOKED) => (),''')
    edit(verifier,'        let login = &mut logins[slot];', '''        let (a,b)=logins.split_at_mut(1);
        let (login,peer)=if slot==0 {(&mut a[0],&mut b[0])} else {(&mut b[0],&mut a[0])};''')
    edit(verifier,'        channels[slot].poll(login);','        channels[slot].poll(login,peer);')
    package='novaOS/user/novaauth/service/exec_package_read.rs'
    edit(package,'''                if self.executable_request.as_ref().is_some_and(|r|r[24..56]==q[8..40]) {''',
         '''                // Même retrait filtré par session que le VFS historique.
                // Les écritures déjà engagées gardent leur reçu/issue propre.
                self.retire_folder(0,Some(&q[8..40]))?;
                self.retire_folder(1,Some(&q[8..40]))?;
                if self.executable_request.as_ref().is_some_and(|r|r[24..56]==q[8..40]) {''')
