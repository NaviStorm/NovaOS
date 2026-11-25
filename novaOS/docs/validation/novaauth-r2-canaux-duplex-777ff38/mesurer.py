"""Contrôle avant démarrage. rustc émet les constantes des types de l'export invité."""
import hashlib,json,re,struct,subprocess,sys
from pathlib import Path
root=Path(sys.argv[1]).resolve()
out=Path(sys.argv[2]).resolve()
out.mkdir(exist_ok=True)
rustc=Path.home()/'.rustup/toolchains/stable-aarch64-apple-darwin/bin/rustc'
def declaration(path,name):
    raw=(root/path).read_text()
    return re.search(r'(?:pub\(super\) )?struct '+name+r'\s*\{[^}]+\}',raw).group().replace('pub(super)','pub')
pool=root/'novaOS/kernel/src/sched/process/duplex_file_pool.rs'
open_file=declaration('novaOS/user/novafs/files/exec_files_lab.rs','OpenFile')
exchange=declaration('novaOS/user/novaauth/service/membership_io_vfs.rs','Exchange')
src='#![no_std]\nmod pool {\n'+pool.read_text()+'\n}\n'+open_file+'\n'+exchange+'\n#[unsafe(no_mangle)] pub static LAYOUT:[usize;3]=[size_of::<pool::FilePool>(),size_of::<OpenFile>(),size_of::<Exchange>()];\nuse core::mem::size_of;\n'
(out/'layout.rs').write_text(src)
for target in ('x86_64-unknown-none','aarch64-unknown-none-softfloat'):
    ll=out/(target+'.ll')
    cmd=[str(rustc),'--edition','2024','-A','dead_code','--target',target,'--crate-type=lib','--emit=llvm-ir','-o',str(ll),str(out/'layout.rs')]
    subprocess.run(cmd,check=True)
    match=re.search(r'@LAYOUT = .*?c"([^"]+)"',ll.read_text())
    data=bytes.fromhex(match.group(1).replace('\\','')) if all(c in '\\0123456789ABCDEFabcdef' for c in match.group(1)) else None
    if data is None: # LLVM prints printable bytes as ASCII.
        chars=iter(re.findall(r'\\[0-9A-Fa-f]{2}|.',match.group(1)))
        data=bytes(int(c[1:],16) if len(c)==3 else ord(c) for c in chars)
    pool_size,file_size,exchange_size=struct.unpack('<QQQ',data)
    total=4026+pool_size+2*(file_size-40)+(exchange_size-224)
    result=dict(target=target,pool=pool_size,open_file=file_size,exchange=exchange_size,base=4026,total=total,marge=4096-total,sha256_declarations=hashlib.sha256(src.encode()).hexdigest(),compilateur=subprocess.check_output([rustc,'-Vv'],text=True).strip())
    (out/(target+'.json')).write_text(json.dumps(result,indent=2)+'\n')
    print(target,total,pool_size,file_size,exchange_size,flush=True)
    if total>4096: raise SystemExit('Budget dépassé')
