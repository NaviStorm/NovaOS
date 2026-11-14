"""Instrumenter une archive neuve du prototype SG, puis tester le vrai cycle de DMA sous QEMU."""
import argparse
import json
from pathlib import Path
import re
import sys
import tempfile


def instrumenter(archive):
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
    changer(syscall, "    let livraison = ipc::termine(cap, tag, op, resultat, |s| {",
            """    ipc::incident_livraison();
    let livraison = ipc::termine(cap, tag, op, resultat, |s| {""")
    changer(bloc/"arena.rs", "        let garde = crate::sched::Guard::new();",
            """        crate::sched::process::ipc::incident_livraison();
        let garde = crate::sched::Guard::new();""")
    ipc = syscall.parent/"ipc"
    (ipc/"probes/livraison.rs").write_bytes(Path(__file__).with_name("dma_livraison_invite.rs").read_bytes())
    with (ipc/"probes.rs").open("a") as f:
        f.write("\nmod livraison;\npub(crate) fn essai_livraison() { livraison::exercice(); }\n")
    with (ipc/"mod.rs").open("a") as f:
        f.write("\npub(crate) use probes::essai_livraison;\npub(crate) use bulk::incident_livraison;\n")
    with (ipc/"bulk.rs").open("a") as f:
        f.write("""
static FAUTE_LIVRAISON: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
static CAP_LIVRAISON: core::sync::atomic::AtomicU64 = core::sync::atomic::AtomicU64::new(0);
pub(super) fn armer_livraison(faute: u64, cap: u64) {
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
pub(super) fn constat_livraison(pid: Pid, cap: u64) -> (usize, usize, bool, bool) {
    with(|s| {
        let node = s.ipc.lookup(pid, cap, SEND | RECEIVE).unwrap();
        let r = s.ipc.ends.get(node.endpoint.slot()).bulk.unwrap().reservation.unwrap();
        (r.index, r.effectues, r.en_vol, r.retrait)
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


def executer(archive, arch):
    sys.path.insert(0, str(archive/"novaOS/tests"))
    from bloc import Bench
    for secteur in (512, 4096):
        with tempfile.TemporaryDirectory() as tmp:
            b = Bench(Path(tmp), arch, secteur)
            raw = bytearray(1024 * 1024)
            raw[8192:16384] = bytes(0x71 + i % 83 for i in range(8192))
            b.image.write_bytes(raw)
            log = b.run("dma-qual", label="ownership", timeout=300)
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
            portails = [("storage-read", "DMA secteur revoque trame inchangee"),
                       ("storage-page-read", "DMA page revoquee 4096 octets inchanges")]
            for scenario, marque in portails:
                trace = b.run(scenario, label="ownership-"+scenario)
                assert marque in trace and "retrait=Ok(())" in trace and "BLOC session terminee ok=false" in trace, trace[-2100:]
                assert b.image.read_bytes() == raw
            print(json.dumps(dict(arch=arch, secteur=secteur, injections=injections,
                baseline_machine=final, sessions=64, cadres_session=session, image_identique=True,
                cas_lot=cas_lot, portails_refuses=[nom for nom, _ in portails])), flush=True)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("archive", type=Path)
    p.add_argument("operation", choices=["instrumenter","aarch64","x86_64"])
    args=p.parse_args()
    archive=args.archive.resolve()
    if args.operation=="instrumenter":
        if archive == Path(__file__).resolve().parents[2] or not (archive/"novaOS/kernel/src/hardware/block/arena.rs").is_file():
            p.error("une archive SG jetable distincte du dépôt est requise")
        instrumenter(archive)
    else:
        executer(archive, args.operation)
