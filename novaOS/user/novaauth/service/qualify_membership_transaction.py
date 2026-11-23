"""Compile le coordinateur réel avec doubles explicites des RPC et du stockage.
Aucun résultat de ce harnais n'est une preuve QEMU ou de persistance réelle.
"""
from pathlib import Path
import argparse,json,hashlib

def prepare(source,destination):
    source=Path(source).resolve();destination=Path(destination)
    destination.mkdir(parents=True,exist_ok=False);(destination/'src').mkdir()
    files=[source/'novaOS/user/novaauth/service/src/bin/common/membership_transaction.rs',source/'novaOS/user/novaauth/service/membership_transaction_tests.rs']
    body=files[0].read_text().replace('//!','//')
    mock=Path(__file__).with_name('membership_transaction_mock.rs').read_text()
    (destination/'src/lib.rs').write_text(mock+'\nmod coordinator {\n'+body+'\n#[cfg(test)] mod tests {\n'+files[1].read_text()+'\n}\n}\n')
    (destination/'Cargo.toml').write_text('''[package]
name="membership-transaction-native"
version="0.0.0"
edition="2024"
[workspace]
[dependencies]
ed25519-dalek={version="=2.2.0",default-features=false}
'''+f'novaos-identity={{path={json.dumps(str(source/"novaOS/user/novaauth"))},features=["administration-lab"]}}\n'+f'novaos-recovery={{path={json.dumps(str(source/"novaOS/user/novaauth/recuperation"))},features=["administration-lab"]}}\n')
    files+=[Path(__file__),Path(__file__).with_name('membership_transaction_mock.rs')]
    (destination/'sources.json').write_text(json.dumps({str(p.relative_to(source)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files},indent=2)+'\n')
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('source',type=Path);p.add_argument('destination',type=Path);a=p.parse_args();prepare(a.source,a.destination)
