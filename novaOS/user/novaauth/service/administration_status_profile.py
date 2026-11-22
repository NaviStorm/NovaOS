"""Marqueurs de statut du seul banc : DEBUG_WRITE existant, quota inchangé.
Les données des ELF restent sur leurs flux collectés ; aucun droit supplémentaire.
"""
from pathlib import Path


def install(root):
    p=Path(root)/'components/shell/script.rs';s=p.read_text()
    old=r'''            "echo" => {
                console::printable(tty::OUTPUT, &args.join(" "))?;
                console::write(tty::OUTPUT, "\n")?;
                0
            }'''
    new=r'''            "echo" => {
                let text=args.join(" ");
                if args.len()==1 && r2_status_marker(&text) {
                    // Le PID vient du noyau. Refuser toute erreur/quota DEBUG,
                    // sans reconstruire ni inventer un statut côté hôte.
                    rt::value(rt::call(rt::DEBUG_WRITE,text.as_ptr() as u64,text.len() as u64,0))?;
                } else {
                    console::printable(tty::OUTPUT, &text)?;
                    console::write(tty::OUTPUT, "\n")?;
                }
                0
            }'''
    if s.count(old)!=1:raise ValueError('ancrage echo R2 absent ou ambigu')
    p.write_text(s.replace(old,new)+HELPER)


HELPER=r'''
fn r2_status_marker(text:&str)->bool {
    if text.len()>64 {return false;}
    let Some(body)=text.strip_prefix("NXR2_").and_then(|s|s.strip_suffix("__FIN")) else {return false;};
    let Some((step,status))=body.split_once('_') else {return false;};
    !step.is_empty() && !status.is_empty()
        && step.bytes().all(|b|b.is_ascii_digit())
        && status.bytes().all(|b|b.is_ascii_digit())
}
'''
