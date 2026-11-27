// Module du seul noyau de test construit dans l'extraction privée de 4F.
// Les acteurs sont trois processus distincts. Ils ne reçoivent aucun bail NIC.
pub(crate) fn run_example() {
    let initial = heap::audit(true);
    let slots = with(|s| s.audit());
    let sans_lien = spawn::launch_thread_witness(&[]).unwrap();
    assert_eq!(sans_lien.join(), Outcome::Exit(0));

    // Un paramètre de lancement matériel du profil VirtIO doit être refusé.
    let inapte = spawn::launch_thread_witness(&3u64.to_le_bytes()).unwrap();
    assert_eq!(inapte.join(), Outcome::Exit(REFUSED));

    for (boite, cas) in [
        (true, 0),
        (false, 0),
        (true, 1),
        (false, 2),
        (true, 3),
        (false, 3),
    ] {
        let ([driver, service, client], retour_pilote) = {
            let _guard = Guard::new();
            let avant_pilote = heap::audit(false).free_pages;
            let d = create_image(spawn::thread_witness(), &[], None, None).unwrap();
            let apres_pilote = heap::audit(false).free_pages;
            let s = create_image(
                include_bytes!(concat!(env!("OUT_DIR"), "/ethernet_port.bin")),
                &[],
                None,
                None,
            )
            .unwrap();
            let c = create_image(
                include_bytes!(concat!(env!("OUT_DIR"), "/ethernet_client.bin")),
                &[],
                None,
                None,
            )
            .unwrap();
            let retour_pilote = with(|k| {
                let (front, client) =
                    liaison_ethernet::installer(k, s.0, c.0, boite, true).unwrap();
                let (back, driver) =
                    liaison_ethernet::installer(k, s.0, d.0, boite, false).unwrap();
                let boot = crate::boot_id::current().unwrap();
                let inc = k
                    .processes
                    .get(c.0)
                    .instance
                    .identity(Some(boot))
                    .unwrap()
                    .instance;
                let epoch = k.processes.get(s.0).instance.serial;
                let set = |pid: Pid, words: &[(usize, u64)]| {
                    heap::with_pages(|m| {
                        let espace = k.processes.get(pid).space.as_ref().unwrap();
                        for &(i, v) in words {
                            espace.write_data(m, i, v);
                        }
                        Ok(())
                    })
                    .unwrap();
                };
                set(
                    c.0,
                    &[
                        (1, client[0]),
                        (2, client[1]),
                        (6, epoch),
                        (11, cas),
                        (12, 288),
                    ],
                );
                set(
                    s.0,
                    &[
                        (1, front[0]),
                        (2, front[1]),
                        (3, back[0]),
                        (4, back[1]),
                        (5, c.0.number),
                        (6, epoch),
                        (7, u64::from_le_bytes(boot[..8].try_into().unwrap())),
                        (8, u64::from_le_bytes(boot[8..].try_into().unwrap())),
                        (9, front[0]),
                        (16, cas),
                        (17, 7),
                        (18, inc),
                    ],
                );
                set(
                    d.0,
                    &[
                        (0, 0x4532_3430_3053_494d),
                        (2, epoch),
                        (5, c.0.number),
                        (7, u64::from_le_bytes(boot[..8].try_into().unwrap())),
                        (8, u64::from_le_bytes(boot[8..].try_into().unwrap())),
                        (9, front[0]),
                        (18, inc),
                        (14, driver[0]),
                        (15, driver[1]),
                        (16, cas),
                    ],
                );
                let roots = [c.0, s.0, d.0].map(|p| k.ipc.racines_pilotes(p));
                let boxes = k.ipc.pages_bulk_count();
                assert_eq!(roots, [0, 4, 0]);
                assert_eq!(boxes, if boite { 2 } else { 0 });
                let (pages, stack) = k.processes.get(d.0).space.as_ref().unwrap().page_counts();
                hal::log(format_args!(
                    "E2400 5A montage boite={boite} cas={cas} racines={roots:?} boites={boxes} ipc_pages={} pilote_pages={pages} pile_pages={stack} cout_creation_pilote={} libres={} dma=0",
                    k.ipc.pages(),
                    avant_pilote - apres_pilote,
                    heap::audit(false).free_pages
                ));
                back
            });
            ([d, s, c], retour_pilote)
        };
        let issue_pilote = driver.join();
        if cas == 3 {
            // Le propriétaire des racines backend est le service. Le superviseur
            // de 4F les révoque au décès constaté de l'adaptateur.
            with(|k| liaison_ethernet::retirer(k, service.0, retour_pilote));
        }
        let issues = [client.join(), service.join(), issue_pilote];
        hal::log(format_args!(
            "E2400 5A issues client/service/pilote={issues:?} boite={boite} cas={cas}"
        ));
        assert_eq!(
            issues,
            [
                Outcome::Exit(0),
                Outcome::Exit(0),
                Outcome::Exit(if cas == 3 { CANCELLED } else { 0 })
            ]
        );
        assert!(with(|s| s.processes.slots.iter().all(Option::is_none)
            && s.audit() == slots
            && s.ipc.pages_bulk_count() == 0));
        assert_eq!(heap::audit(true), initial);
        if cas == 3 {
            hal::log(format_args!(
                "E2400 5A perte boite={boite} prefixe=4 inconnus=4 annules=4 ACK=refuse restitution_CPU=OK libres={}",
                initial.free_pages
            ));
        } else {
            hal::log(format_args!(
                "E2400 5A port v1 boite={boite} trames=288 lots=1..8 cas={cas} restitution=OK libres={}",
                initial.free_pages
            ));
        }
    }
    hal::log(format_args!("E2400 5A Q35 CPU termine"));
}
