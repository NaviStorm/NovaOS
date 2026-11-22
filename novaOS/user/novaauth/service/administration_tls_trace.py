"""Diagnostic TLS sans secret, activé seulement dans le témoin R2."""
from pathlib import Path

def install(root):
    root=Path(root)
    p=root/'novaOS/user/novaauth/service/src/bin/common/recovery_bridge.rs'
    s=p.read_text().replace('fn io(_:u64)->TlsError {TlsError::IoError}', 'fn io(code:u64)->TlsError {NATIVE.store(code,core::sync::atomic::Ordering::Relaxed);TlsError::IoError}')
    s=s.replace('    canal::dialogue(key,|tls,until| {','    NATIVE.store(0,core::sync::atomic::Ordering::Relaxed);\n    let mut op=0u16;let mut sequence=0u32;let mut stage=0u64;\n    let mut marks=[(0u16,0u32,0u64,0u64);3];let mut count=0usize;\n    let result=canal::dialogue(key,|tls,until| {')
    s=s.replace('            let first=match initial.take()', '            stage=1;\n            let first=match initial.take()')
    s=s.replace('            let h=rpc::receive', '            stage=2;\n            let h=rpc::receive')
    s=s.replace('            let n=usize::from(h.size);','            op=h.operation;sequence=h.sequence;\n            let began=canal::diagnostic_clock();\n            marks[count%3]=(op,sequence,began,0);\n            count+=1;stage=3;\n            let n=usize::from(h.size);')
    s=s.replace('            bound(until).map_err(io)?;', '            stage=4;\n            bound(until).map_err(io)?;\n            stage=5;')
    s=s.replace('                &answer[HEADER..HEADER+size],||bound(until)).map_err(io)?;',
                '                &answer[HEADER..HEADER+size],||bound(until)).map_err(io)?;\n            marks[(count-1)%3].3=canal::diagnostic_clock();')
    s=s.replace('    }).map_err(|_|rt::REFUSED)?;', '    });\n    // Trois derniers échanges, sans charge utile ni identifiant de récupération.\n    for i in count.saturating_sub(3)..count {\n        let (o,q,b,e)=marks[i%3];trace(format_args!("R2TLS op={} seq={} debut={} fin={}",o,q,b,e));\n    }\n    if let Err(error)=result {\n        trace(format_args!("R2TLS echec stage={} op={} seq={} native={}",stage,op,sequence,NATIVE.load(core::sync::atomic::Ordering::Relaxed)));\n        trace(format_args!("R2TLS erreur={:?}",error));\n        return Err(rt::REFUSED);\n    }')
    s+=TRACE
    p.write_text(s)
    p=root/'novaOS/user/novaauth/service/src/bin/common/canal.rs';s=p.read_text()
    s+='\npub(super) fn diagnostic_clock()->u64 {clock().unwrap_or(u64::MAX)}\n'
    s=s.replace('    tls.open(TlsContext::new(&cfg, &mut p))?;',
                '    tls.open(TlsContext::new(&cfg, &mut p)).map_err(|e|{crate::recovery_bridge::trace(format_args!("R2TLS handshake={:?}",e));e})?;')
    p.write_text(s)

    # Le secteur de transport P9/4096 était sérialisé par 43 trames PIECE
    # à chaque fragment TLS. Réutiliser la voie PAGE déjà négociée et déléguée.
    p=root/'novaOS/user/novafs/device.rs';s=p.read_text()
    old='        block.transfer(sector, page * (4096 / size) as u64, write)'
    if s.count(old)!=1:raise ValueError('ancrage transport hors P9 ambigu')
    p.write_text(s.replace(old, BULK))

BULK = r'''        let first=page * (4096 / size) as u64;
        let cap=crate::runtime::config(10);
        if size==4096 && cap!=0 {
            // read_page/write_page conservent la négociation FEATURE_PAGE et
            // le repli historique ; aucun handle ni droit supplémentaire.
            let buffer:&mut [u8;4096]=sector.try_into().map_err(|_|crate::runtime::BAD_ARGUMENT)?;
            if write {block.write_page(first,cap,buffer)}
            else {block.read_page(first,cap,buffer)}
        } else {block.transfer(sector,first,write)}'''

TRACE=r'''
static NATIVE:core::sync::atomic::AtomicU64=core::sync::atomic::AtomicU64::new(0);
pub(crate) fn trace(args:core::fmt::Arguments) {
    use core::fmt::Write;
    struct Buffer {data:[u8;128],n:usize}
    impl core::fmt::Write for Buffer {
        fn write_str(&mut self,s:&str)->core::fmt::Result {
            let out=self.data.get_mut(self.n..self.n+s.len()).ok_or(core::fmt::Error)?;
            out.copy_from_slice(s.as_bytes());self.n+=s.len();Ok(())
        }
    }
    let mut b=Buffer{data:[0;128],n:0};
    if b.write_fmt(args).is_ok() {common::marker(&b.data[..b.n]);}
}
'''
