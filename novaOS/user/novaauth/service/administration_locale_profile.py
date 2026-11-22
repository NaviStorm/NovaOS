"""Locale et man des ELF R2, uniquement dans une copie de qualification QEMU.

Le drapeau 2 NXADMIN1/NXEXEC01 existe déjà. Aucun descripteur ou droit nouveau.
Les octets privés nouvellement utilisés n'acceptent que 0 ou 2 ; les anciens
clients à réservés nuls gardent leur comportement anglais.
"""
from pathlib import Path


def install(root):
    root = Path(root)

    def edit(name, before, after):
        p = root / name
        text = p.read_text()
        if text.count(before) != 1:
            raise ValueError(f'ancrage locale R2 absent ou ambigu : {name}: {before[:64]}')
        p.write_text(text.replace(before, after))

    edit('components/shell/session_auth_lab.rs', '    identity: [u8; 32],',
         '    identity: [u8; 32],\n    pub(super) french: bool,')
    edit('components/shell/session_auth_lab.rs', '        begin[119] = role;',
         '        begin[119] = role;\n        begin[112] = if self.french {2} else {0};')
    edit('components/shell/script.rs', '        match cmd {', '''        self.login.french = matches!(f.vars.get("LANG").map(|(tag,_)|tag.as_str()),
            Some("fr" | "fr-FR" | "fr-CA"));
        match cmd {''')
    edit('components/shell/script.rs', '            if name == "man" {', '''            if name == "man" {
                // Résoudre et autoriser normalement l'ELF. Aucun catalogue de
                // commandes administratives n'est compilé dans nsh.
                if let [app] = literal.as_slice()
                    && nova_cli::documentation::lookup(app).is_none() {
                    return self.login.execute(app, &["man"]).map(Flow::Status);
                }''')
    path = 'novaOS/user/novaauth/service/src/bin/common/shell_verifier.rs'
    edit(path, '                    || msg[120..127] != [0; 7]',
         '                    || !matches!(msg[120], 0 | 2)\n                    || msg[121..127] != [0; 6]')
    edit(path, '                q[112] = msg[127];',
         '                q[112] = msg[127];\n                q[113] = msg[120];')
    path = 'novaOS/kernel/src/sched/process/ipc/exec_snapshot_lab.rs'
    edit(path, '    role: u8,', '    role: u8,\n    locale: u8,')
    edit(path, '            || q[113..120] != [0; 7]',
         '            || !matches!(q[113], 0 | 2)\n            || q[114..120] != [0; 6]')
    edit(path, '            role: q[112],', '            role: q[112],\n            locale: q[113],')
    edit(path, '            config[48..56].copy_from_slice(&(a.argv_len as u64).to_le_bytes());',
         '            config[8..16].copy_from_slice(&(a.locale as u64).to_le_bytes());\n            config[48..56].copy_from_slice(&(a.argv_len as u64).to_le_bytes());')
