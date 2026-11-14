"""Instrumenter une archive neuve du prototype SG, puis tester le vrai cycle de DMA sous QEMU."""
import argparse
from contextlib import contextmanager
import json
from pathlib import Path
import re
import sys
import tempfile


def instrumenter(archive, *, exiger_retrait=False):
    ipc_lot = archive / "novaOS/kernel/src/sched/process/ipc/bulk.rs"
    retrait_immediat = "fn abandon_sur_attente(" in ipc_lot.read_text()
    if exiger_retrait and not retrait_immediat:
        raise ValueError("Le profil demandé ne contient pas le correctif de retrait")
    bloc = archive / "novaOS/kernel/src/hardware/block"

    def changer(path, avant, apres):
        source = path.read_text()
        if source.count(avant) != 1:
            raise ValueError(f"point d’injection absent ou ambigu dans {path.name}: {avant!r}")
        path.write_text(source.replace(avant, apres))

    def avant(fichier, marque, ident):
        changer(bloc/fichier, marque, f"super::lab_propriete::point({ident})?;\n        " + marque)

    def apres(fichier, marque, ident):
        changer(bloc/fichier, marque, marque + f"\n        super::lab_propriete::point({ident})?;")

    (bloc/"lab_propriete.rs").write_bytes(Path(__file__).with_name("dma_propriete_invite.rs").read_bytes())
    mod = bloc/"mod.rs"
    s = mod.read_text()
    a = s.index("    let res = (|| {", s.index("pub(crate) fn probe()"))
    b = s.index('    hal::log(format_args!("DMA bilan', a)
    texte = s[:a]+"    let res = lab_propriete::essai();\n"+s[b:]
    mod.write_text(texte.replace("mod arena;", "mod arena;\nmod lab_propriete;", 1))
    with (archive/"novaOS/kernel/src/memory/mod.rs").open("a") as f:
        f.write("\nimpl Memory { pub(crate) fn dma_laboratoire(&self) -> u64 { self.dma_frames } }\n")
    with (archive/"novaOS/kernel/src/arch/aarch64/timer.rs").open("a") as f:
        f.write("""\npub(crate) fn spi_ownership() {
    write(DIST, 0x204, 1 << (disk_id()-32)); unsafe { asm!("dsb sy", "isb", options(nostack, preserves_flags)); }
}
pub(crate) fn spi_ownership_actif() -> bool { read(DIST, 0x104) & (1 << (disk_id()-32)) != 0 }
""")
    with (bloc/"arena.rs").open("a") as f:
        f.write("""\nimpl Transport {
    pub(super) fn laboratoire_arene(&mut self, sens: bool, pages: usize) -> Result<()> {
        self.arena.assurer(&mut self.unit, &self.p, sens, pages)
    }
}\n""")
    avant("io.rs", "memory::pin_dma()", 1)
    avant("ownership.rs", "r.unit = Some(domain::Domain::preparer", 2)
    apres("ownership.rs", "unit.start(&r.p)?;", 3)
    avant("ownership.rs", "unit.map(&r.p, n, r.p.0[16 + n].pa, *droit)?;", 4)
    apres("ownership.rs", "let (sectors, size) = d.geometry()?;", 5)
    apres("ownership.rs", "unit.route(&r.p, d, true)?;", 6)
    avant("ownership.rs", "USED_ADDR.store((r.p.0[18]", 7)
    apres("ownership.rs", "d.ready()?;", 8)
    apres("ownership.rs", "unit.faults(&r.p)?;", 9)
    avant("ownership.rs", "q.completed()?;", 10)
    apres("ownership.rs", "self.d.reset()?;", 11)
    apres("ownership.rs", "unit.route(&self.p, self.d, false)?;", 12)
    avant("ownership.rs", "unit.clear_pending(self.d)?;", 13)
    avant("ownership.rs", "unit.retirer_slot(&self.p, n)?;", 14)
    avant("ownership.rs", "unit.stop(&self.p)?;", 15)
    avant("arena.rs", "unite.map(p, slot(ecrire, i), f.pa,", 16)
    avant("arena.rs", "unite.retirer_slot(p, slot(sens == 1, self.retraits[sens]))?;", 17)
    apres("viommu.rs", "self.requests = Some(Queue::new(unit, 0, a, a.map(|f| f.pa))?);", 30)
    apres("viommu.rs", "self.events = Some(Queue::new(unit, 1, b, b.map(|f| f.pa))?);", 31)
    apres("viommu.rs", "unit.ready()?;", 32)
    changer(bloc/"viommu.rs", "        self.vol = Some((op, res));",
            """        self.vol = Some((op, res));
        let id = match op { 5 => 33, 1 => 34, 4 => 36, 2 => 38, _ => 0 };
        if id != 0 { super::lab_propriete::point(id)?; }""")
    changer(bloc/"viommu.rs", "match self.map(p, n, 0, 0) {",
            "match super::lab_propriete::point(37).map_err(|_| Error::Fault).and_then(|_| self.map(p,n,0,0)) {")
    changer(bloc/"viommu.rs", "self.request(p, 36, 4)",
            "super::lab_propriete::point(39).map_err(|_| Error::Fault)?;\n            self.request(p,36,4)")
    apres("vtd.rs", "self.phase = Installation::Interruptions;", 35)
    apres("vtd.rs", "self.regs.w64(0x88, u64::from(self.tail));", 33)

    # Le processus possède ses tampons. La sonde supprime uniquement son propre
    # bail après la lecture et avant la copie vers son espace utilisateur.
    syscall = archive / "novaOS/kernel/src/sched/process/syscall.rs"
    changer(syscall, "    block::command(pid.number, key, &mut wire)?;",
            """    block::command(pid.number, key, &mut wire)?;
    if block::temoin_refus_portail() && wire[1] == block::wire::READ && wire[2] == block::wire::PIECE {
        block::died(pid.number);
    }""")
    changer(syscall, "    block::page_command(pid.number, key, &w, &mut page)?;",
            """    block::page_command(pid.number, key, &w, &mut page)?;
    if block::temoin_refus_portail() && w[1] == block::wire::READ { block::died(pid.number); }""")
    retrait_automatique = "doute.then_some(key)" in syscall.read_text()
    signature = ("resultat, doute.then_some(key)" if retrait_automatique
                 else "resultat, doute" if retrait_immediat else "resultat")
    livraison = f"    let livraison = ipc::termine(cap, tag, op, {signature}, |s| {{"
    changer(syscall, livraison, "    ipc::incident_livraison();\n" + livraison)
    changer(bloc/"arena.rs", "        let garde = crate::sched::Guard::new();",
            """        crate::sched::process::ipc::incident_livraison();
        let garde = crate::sched::Guard::new();""")
    ipc = syscall.parent/"ipc"
    (ipc/"probes/livraison.rs").write_bytes(Path(__file__).with_name("dma_livraison_invite.rs").read_bytes())
    with (ipc/"probes.rs").open("a") as f:
        f.write("\nmod livraison;\npub(crate) fn essai_livraison() { livraison::exercice(); }\n")
    with (ipc/"mod.rs").open("a") as f:
        f.write("\npub(crate) use probes::essai_livraison;\npub(crate) use bulk::incident_livraison;\n")
    if retrait_automatique:
        (ipc/"probes/livraison").mkdir()
        (ipc/"probes/livraison/retrait_session.rs").write_bytes(
            Path(__file__).with_name("dma_retrait_session_invite.rs").read_bytes())
        sonde = ipc/"probes/livraison.rs"
        changer(sonde, "pub(super) fn exercice() {", "mod retrait_session;\npub(super) fn exercice() {")
        marque = '                "DMA livraison multi={multi} faute={faute:?} avance={effectues} kicks=1"\n            ));\n        }\n    }\n}'
        changer(sonde, marque, marque[:-1] + "    retrait_session::exercice();\n}")
        changer(mod, "        let used = self.q.completed()?;",
                 "        lab_propriete::point(40)?;\n        let used = self.q.completed()?;")
        with (bloc/"lab_propriete.rs").open("a") as f:
            f.write("""
static ACQUITS_DOUBLES: AtomicUsize = AtomicUsize::new(0);
pub(crate) fn perdre_livraison_et_drainage() {
    ACQUITS_DOUBLES.store(1, SeqCst);
    armer(40, 1);
}
pub(crate) fn acquits_temoin_perdus() -> bool { ACQUITS_DOUBLES.load(SeqCst) == 3 }
pub(crate) fn cadres_temoin() -> u64 { cadres() }
""")
        changer(bloc/"lab_propriete.rs", "        DECLENCHES.fetch_add(1, SeqCst);",
                """        DECLENCHES.fetch_add(1, SeqCst);
        if id == 40 && ACQUITS_DOUBLES.load(SeqCst) == 1 {
            ACQUITS_DOUBLES.store(2, SeqCst);
            armer(10, 1);
        } else if id == 10 && ACQUITS_DOUBLES.load(SeqCst) == 2 {
            ACQUITS_DOUBLES.store(3, SeqCst);
        }""")
        with mod.open("a") as f:
            f.write("pub(crate) use lab_propriete::{cadres_temoin, perdre_livraison_et_drainage, acquits_temoin_perdus};\n")
        with (ipc/"bulk.rs").open("a") as f:
            f.write("""
pub(super) fn constat_quarantaine(pilote: Pid, cap: u64, moteur: Pid, table: u64,
                                  tag: u64, n: u64, flush: bool, retrait: Option<bool>) {
    with(|s| {
        let ipc = &s.ipc;
        let data = ipc.lookup(pilote, cap, SEND | RECEIVE).unwrap().endpoint;
        let offsets = ipc.lookup(moteur, table, MANAGE).unwrap().endpoint;
        let voie = ipc.ends.get(data.slot()).bulk;
        let table = ipc.ends.get(offsets.slot()).bulk;
        if let Some(retrait) = retrait {
            let session = ipc.session_lots.unwrap();
            assert_eq!((session.donnees.endpoint, session.offsets.endpoint), (data, offsets));
            assert!(table.is_some());
            let lane = voie.unwrap();
            let r = lane.reservation.unwrap();
            assert_eq!((r.tag,r.n,r.moteur,r.retrait,r.en_vol,r.doute),
                       (tag,n as usize,moteur,retrait,false,true));
            assert_eq!((r.effectues,r.index),(usize::from(flush),usize::from(flush)));
            assert_ne!(&ipc.pages_bulk.get(lane.slot)[..8], &[0;8]);
        } else {
            assert!(voie.is_none() && table.is_none() && ipc.session_lots.is_none());
        }
    });
}
""")
    if retrait_immediat:
        # Ces points se trouvent après l’acquittement du périphérique et avant
        # la restitution. Observer les crédits avant que la réservation disparaisse.
        for fichier, marge in ((ipc/"bulk.rs", "            "),
                               (ipc/"bulk/multipage.rs", "                ")):
            marque = marge + "s.ipc.pages_bulk.get_mut(lane.slot)[..8 * r.n].fill(0);"
            appel = "marquer_cloture(r);" if fichier.name == "bulk.rs" else "super::marquer_cloture(r);"
            changer(fichier, marque + "\n" + marge + "lane.reservation = None;",
                    marge + appel + "\n" + marque + "\n" + marge + "lane.reservation = None;")
    with (ipc/"bulk.rs").open("a") as f:
        f.write("pub(super) const RETRAIT_IMMEDIAT: bool = " + str(retrait_immediat).lower() + ";\n")
        f.write("""
static FAUTE_LIVRAISON: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static CAP_LIVRAISON: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static CLOTURES_OBSERVEES: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
fn marquer_cloture(r: &Reservation) {
    assert_eq!((r.tag, r.en_vol, r.index, r.effectues), (71, false, 0, 0));
    assert_eq!(CLOTURES_OBSERVEES.fetch_add(1, Ordering::SeqCst), 0);
}
pub(super) fn armer_livraison(faute: u64, cap: u64) {
    CLOTURES_OBSERVEES.store(0, Ordering::SeqCst);
    CAP_LIVRAISON.store(cap, Ordering::SeqCst);
    FAUTE_LIVRAISON.store(faute, Ordering::SeqCst);
}
pub(crate) fn incident_livraison() {
    let faute = FAUTE_LIVRAISON.swap(0, Ordering::SeqCst);
    if faute == 0 { return; }
    with(|s| {
        let pid = s.thread(s.current()).owner.unwrap();
        let cap = CAP_LIVRAISON.load(Ordering::SeqCst);
        let node = s.ipc.lookup(pid, cap, SEND | RECEIVE).unwrap();
        let r = s.ipc.ends.get(node.endpoint.slot()).bulk.unwrap().reservation.unwrap();
        assert!(r.en_vol && r.lire);
        match faute {
            1 => crate::hardware::block::died(pid.number),
            2 => assert_eq!(s.ipc.retrait_lot(node, true), Err(WOULD_BLOCK)),
            3 => {
                let attendu = r.base + r.n as u64 * 4096;
                let espace = s.processes.get_mut(r.moteur).space.as_mut().unwrap();
                heap::with_pages(|m| espace.resize_heap(m, attendu, attendu - 4096)).unwrap();
            }
            _ => unreachable!(),
        }
    });
}
pub(super) fn constat_livraison(pid: Pid, cap: u64, n: usize) -> Option<(usize, usize, bool, bool)> {
    with(|s| {
        let node = s.ipc.lookup(pid, cap, SEND | RECEIVE).unwrap();
        let lane = s.ipc.ends.get(node.endpoint.slot()).bulk.unwrap();
        let clotures = CLOTURES_OBSERVEES.load(Ordering::SeqCst);
        if let Some(r) = lane.reservation {
            assert_eq!(clotures, 0);
            Some((r.index, r.effectues, r.en_vol, r.retrait))
        } else {
            assert_eq!(clotures, 1);
            assert!(s.ipc.pages_bulk.get(lane.slot)[..8*n].iter().all(|&octet| octet == 0));
            None
        }
    })
}
""")
    changer(mod, "    let res = lab_propriete::essai();",
            """    let res = lab_propriete::essai();
    if res.is_ok() {
        TEMOIN_REFUS.store(false, Ordering::SeqCst);
        crate::sched::process::ipc::essai_livraison();
    }""")
    with mod.open("a") as f:
        f.write("""
static TEMOIN_REFUS: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);
pub(crate) fn temoin_refus_portail() -> bool { TEMOIN_REFUS.load(Ordering::SeqCst) }
""")
    disque = archive / "novaOS/user/storage/disk.rs"
    changer(disque, "        let result = call(wire::PORTAL, key, (&raw mut r) as u64, 128);",
            """        let avant = r.0;
        let result = call(wire::PORTAL, key, (&raw mut r) as u64, 128);
        if result.status == REVOKED && avant[1] == wire::READ && avant[2] == wire::PIECE {
            if r.0 != avant { return Err(REFUSED); }
            let msg = b"DMA secteur revoque trame inchangee";
            call(DEBUG_WRITE, msg.as_ptr() as u64, msg.len() as u64, 0);
        }""")
    changer(disque, "let page = if lane != 0 {", "let page = if lane != 0 || config(1) == 10 {")
    changer(disque, "    let mut page = page;", """    let mut page = page;
    if config(1) == 10 {
        let data = page.as_deref_mut().ok_or(DENIED)?;
        data.fill(0x5a);
        let mut r = Request(wire::blank(wire::READ));
        r.0[2] = wire::PAGE_PHASE;
        wire::put(&mut r.0, 4, 4096);
        r.0[8..16].copy_from_slice(&u64::from(8192 / size).to_le_bytes());
        let recu = call(wire::PAGE_PORTAL, key, (&raw const r) as u64, data.as_mut_ptr() as u64);
        if recu.status != REVOKED || data.iter().any(|octet| *octet != 0x5a) {
            return Err(REFUSED);
        }
        let msg = b"DMA page revoquee 4096 octets inchanges";
        call(DEBUG_WRITE, msg.as_ptr() as u64, msg.len() as u64, 0);
    }""")


@contextmanager
def cellule(archive, arch, secteur, preuves):
    if preuves is None:
        with tempfile.TemporaryDirectory() as dossier:
            yield Path(dossier)
    else:
        from preuves_stockage import dossier_essai, enregistrer, provenance
        preuves.mkdir(parents=True, exist_ok=True)
        with dossier_essai(preuves/"rapport.json", arch, secteur, "propriete", conserver=True) as dossier:
            enregistrer(dossier, "export.json", provenance(archive))
            yield dossier


def executer(archive, arch, *, verifier_drainage=False, preuves=None):
    sys.path.insert(0, str(archive/"novaOS/tests"))
    from bloc import Bench
    for secteur in (512, 4096):
        with cellule(archive, arch, secteur, preuves) as dossier:
            b = Bench(dossier, arch, secteur)
            raw = bytearray(1024 * 1024)
            raw[8192:16384] = bytes(0x71 + i % 83 for i in range(8192))
            b.image.write_bytes(raw)
            if preuves is not None:
                from preuves_stockage import figer_initiale
                figer_initiale(b.image)
            log = b.run("dma-qual", label="ownership", timeout=300, diagnostic=preuves is not None)
            ligne = re.search(r"DMA OWNERSHIP injections=(\d+) cycles=64 final=(\d+) session=(\d+) allocations=(\d+) pertes=0", log)
            assert ligne and "DMA bilan Ok(())" in log and b.image.read_bytes() == raw, log[-2400:]
            injections, final, session, allocations = map(int, ligne.groups())
            assert final == (0 if arch=="aarch64" else 9) and session == (16 if arch=="aarch64" else 7)
            assert allocations == 16 and injections == (120 if arch=="aarch64" else 112), injections
            cas_lot = [
                f"DMA livraison multi={multi} faute={faute} avance={avance} kicks=1"
                for multi in ("false", "true")
                for faute, avance in [("Aucune", 2 if multi == "true" else 1),
                                      ("Bail", 0), ("Reservation", 0), ("Adresse", 0)]
            ]
            assert all(log.count(marque) == 1 for marque in cas_lot), log[-2500:]
            assert log.count("DMA lecture historique et PAGE integrales") == 1, log[-2500:]
            retraits = re.findall(r"DMA retrait automatique op=(mono|sg|flush) acteurs=2 tag=83 cadres=(\d+)/(\d+)/(\d+) kicks=1 acquit_perdu=1", log)
            if verifier_drainage:
                assert [op for op, *_ in retraits] == ["mono", "sg", "flush"], log[-2500:]
                assert all(int(avant) == final == int(apres) < int(epingles)
                           for _, avant, epingles, apres in retraits)
                fermetures = {(multi, faute) for multi in ("false", "true")
                              for faute in ("Bail", "Reservation", "Adresse")}
                observees = re.findall(r"DMA livraison fermee multi=(false|true) faute=(\w+)", log)
                assert len(observees) == 6 and set(observees) == fermetures
            portails = [("storage-read", "DMA secteur revoque trame inchangee"),
                       ("storage-page-read", "DMA page revoquee 4096 octets inchanges")]
            for scenario, marque in portails:
                trace = b.run(scenario, label="ownership-"+scenario, diagnostic=preuves is not None)
                assert marque in trace and "retrait=Ok(())" in trace and "BLOC session terminee ok=false" in trace, trace[-2100:]
                assert b.image.read_bytes() == raw
            print(json.dumps(dict(arch=arch, secteur=secteur, injections=injections,
                baseline_machine=final, sessions=64, cadres_session=session, image_identique=True,
                cas_lot=cas_lot, retraits_apres_perte_acquit=retraits, portails_refuses=[nom for nom, _ in portails])), flush=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path)
    p.add_argument("operation", choices=["instrumenter","instrumenter-retrait","aarch64","x86_64"])
    p.add_argument("--verifier-drainage", action="store_true",
                   help="exiger le retrait confirmé, les deux acteurs encore vivants et les deux voies restituées")
    p.add_argument("--preuves", type=Path, help="dossier durable propre à une architecture/topologie")
    args=p.parse_args()
    archive=args.archive.resolve()
    if args.operation.startswith("instrumenter"):
        if archive == Path(__file__).resolve().parents[2] or not (archive/"novaOS/kernel/src/hardware/block/arena.rs").is_file():
            p.error("une archive SG jetable distincte du dépôt est requise")
        instrumenter(archive, exiger_retrait=args.operation=="instrumenter-retrait")
    else:
        executer(archive, args.operation, verifier_drainage=args.verifier_drainage, preuves=args.preuves)
