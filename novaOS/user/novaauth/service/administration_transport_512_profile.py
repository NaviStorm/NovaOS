"""Même cellule chiffrée de 512 octets, voie PAGE existante, témoin R2 x86 QEMU."""
from pathlib import Path
import runpy

# Aucun tampon persistant, nouvelle capacité ou négociation. La voie PAGE doit
# déjà être disponible : pas de repli en huit accès aux adresses de transport.
PAGE512 = r'''        #[cfg(target_arch="x86_64")]
        if size==512 && cap!=0 && block.r2_has_page() {
            let mut buffer=[0u8;4096];
            if write {
                buffer[..512].copy_from_slice(sector);
                return block.write_page(first,cap,&buffer);
            }
            block.read_page(first,cap,&mut buffer)?;
            if buffer[512..].iter().any(|&b|b!=0) {
                return Err(crate::runtime::BAD_ARGUMENT);
            }
            sector.copy_from_slice(&buffer[..512]);
            return Ok(());
        }
'''

def install(root):
    root=Path(root)
    p=root/'novaOS/user/novafs/device.rs'
    s=p.read_text()
    old=runpy.run_path(str(root/'novaOS/user/novaauth/service/administration_tls_trace.py'))['BULK']
    if s.count(old)!=1: raise ValueError('ancrage transport PAGE ambigu')
    replacement=old.replace('        if size==4096',PAGE512+'        if size==4096')
    p.write_text(s.replace(old,replacement))
    p=root/'novaOS/user/storage/medium.rs';s=p.read_text()
    anchor='impl Block {'
    if s.count(anchor)!=1: raise ValueError('ancrage négociation PAGE ambigu')
    p.write_text(s.replace(anchor,anchor+'''
    #[cfg(target_arch="x86_64")]
    pub(crate) fn r2_has_page(&self)->bool {self.features & w::FEATURE_PAGE != 0}
'''))
