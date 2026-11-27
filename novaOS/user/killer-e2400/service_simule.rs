//! Adaptateur du protocole privé service/pilote v1 pour le seul pair CPU.
use crate::{backend::*, boucle::Boucle, canal::*, runtime::*};
use ethernet_v1::Binding;

pub fn servir(
    lien: Canal,
    origine: Binding,
    tampons: &mut crate::boucle::Tampons,
    perte_recu: bool,
) -> Result<()> {
    let ouverture = fields(&lien.recevoir(DRIVER)?)?;
    require(ouverture == (OUVRIR, 0))?;
    let mut port = Boucle::nouvelle(origine, tampons).map_err(|_| REFUSED)?;
    lien.envoyer(DRIVER, &pair(OK, 0))?;
    let mut frame = [0u8; FRAME];
    loop {
        let msg = lien.recevoir(DRIVER)?;
        require(msg.len >= 16)?;
        let op = u64::from_le_bytes(msg.payload[..8].try_into().unwrap());
        let arg = u64::from_le_bytes(msg.payload[8..16].try_into().unwrap());
        require(op == PREPARER_LOT || msg.len == 16)?;
        let valeur = match op {
            PREPARER if !lien.boite => {
                let coupon = (arg >> 32) as usize;
                let n = arg as u32 as usize;
                require(coupon < CREDITS && (14..=FRAME).contains(&n))?;
                lien.prendre(1, &mut frame[..n])?;
                port.preparer(coupon, &frame[..n]).map_err(|_| REFUSED)?;
                coupon as u64
            }
            PREPARER_LOT if lien.boite && (1..=8).contains(&arg) => {
                let n = arg as usize;
                require(msg.len as usize == 16 + 4 * n)?;
                let mut ids = [0usize; CREDITS];
                let mut lot = Trames::vide();
                for (i, d) in msg.payload[16..16 + 4 * n].chunks_exact(4).enumerate() {
                    let (id, len) = entree(u32::from_le_bytes(d.try_into().unwrap()));
                    require(id < CREDITS && !ids[..i].contains(&id))?;
                    ids[i] = id;
                    lot.ajouter(len)?;
                }
                let len = lot.longueur();
                lien.recevoir_lot(&mut lot.bytes[..len])?;
                for (i, id) in ids[..n].iter().enumerate() {
                    port.preparer(*id, lot.trame(i)).map_err(|_| REFUSED)?;
                }
                arg
            }
            PUBLIER if (1..=8).contains(&arg) => {
                if perte_recu {
                    // Panne du pair CPU, entre commande et réponse. La propriété
                    // du préfixe est incertaine. Le suffixe n'a pas été soumis.
                    let bilan = port.perdre_recu(arg as usize).map_err(|_| REFUSED)?;
                    require(bilan.tx_incertains == arg as usize)?;
                    return Err(CANCELLED);
                }
                port.publier(arg as usize).map_err(|_| REFUSED)?;
                arg
            }
            COMPLETIONS if lien.boite && (1..=8).contains(&arg) => {
                u64::from(port.completions(arg as usize))
            }
            COMPLETION if !lien.boite && arg == 0 => {
                let masque = port.completions(1);
                if masque == 0 {
                    lien.envoyer(DRIVER, &pair(WOULD_BLOCK, 0))?;
                    continue;
                }
                u64::from(masque.trailing_zeros())
            }
            ECOUTER if !lien.boite && arg == 0 => 0,
            RECEPTION if !lien.boite && arg == 0 => {
                match port.recevoir(&mut frame).map_err(|_| REFUSED)? {
                    None => lien.envoyer(DRIVER, &pair(WOULD_BLOCK, 0))?,
                    Some(n) => {
                        lien.envoyer(DRIVER, &pair(OK, n as u64))?;
                        lien.offrir(1, &frame[..n])?;
                    }
                }
                continue;
            }
            RECEPTION_LOT if lien.boite && arg & !0x1ff == 0 && (1..=8).contains(&(arg & 255)) => {
                let mut lot = Trames::vide();
                let mut longueurs = [0u32; CREDITS];
                let mut count = 0;
                for entree in &mut longueurs[..(arg & 255) as usize] {
                    let Some(n) = port.recevoir(&mut frame).map_err(|_| REFUSED)? else {
                        break;
                    };
                    lot.ajouter(n)?.copy_from_slice(&frame[..n]);
                    *entree = n as u32;
                    count += 1;
                }
                let mut msg = [0u8; 48];
                let n = liste(OK, &longueurs[..count], &mut msg);
                lien.envoyer_lot(&lot.bytes[..lot.longueur()], || {
                    lien.envoyer(DRIVER, &msg[..n])
                })?;
                continue;
            }
            FERMER if arg == 0 => {
                port.fermer().map_err(|_| REFUSED)?;
                return lien.envoyer(DRIVER, &pair(OK, 0));
            }
            _ => return Err(DENIED),
        };
        lien.envoyer(DRIVER, &pair(OK, valeur))?;
    }
}
