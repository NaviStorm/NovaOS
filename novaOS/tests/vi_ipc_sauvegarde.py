"""Vérifier les deux critères MOA sur une seule sauvegarde de référence par VM."""
import argparse
import json
from pathlib import Path
import re


def bilan(journal, mesure):
    text = journal.read_text(errors="strict")
    row, = json.loads(mesure.read_text())
    assert row["octets"] == 4 << 20 and "fichier enregistré" in text
    roles = re.search(r"BLOC chaine novaFS=(\d+) bloc=(\d+) pilote=(\d+) client=(\d+)", text)
    assert roles and roles.groups() == ("2", "1", "3", "4")
    editor, = re.findall(r"CLI spawn image=14 pid=(\d+)", text)
    pids = [1, 2, 3, 4, int(editor)]
    lines = re.findall(r"SAVE from=(\d+) vers=(\[[^\n]+\])", text)
    assert [int(pid) for pid, _ in lines] == pids
    matrix = [json.loads(vector) for _, vector in lines]
    pairs = {(0, 1): "moteur_bloc", (0, 2): "bloc_pilote", (1, 3): "nsh_moteur", (3, 4): "vi_nsh"}
    counts = {}
    for i in range(5):
        assert len(matrix[i]) == 5 and matrix[i][i] == 0
        for j in range(i + 1, 5):
            assert matrix[i][j] == matrix[j][i]
            if (i, j) in pairs:
                counts[pairs[i, j]] = matrix[i][j]
            else:
                assert matrix[i][j] == 0
    end, = re.findall(r"SAVE duree=([^\n]+)", text)
    fields = dict(re.findall(r"(\w+)=(\d+)", "duree=" + end))
    assert int(fields["autres"]) == 0
    (phase, block), = re.findall(r"SAVE phases=(\[[^\]]+\]) bloc=(\[[^\]]+\])", text)
    local, = re.findall(r"SAVE controles_locaux=(\[[^\]]+\])", text)
    controls = dict(zip(("create", "dup", "close", "revoke", "notify", "cancel", "bootstrap"),
                        json.loads(local), strict=True))
    hz = int(re.search(r"BLOC memoire [^\n]* hz=(\d+)", text)[1])
    def ms(cycles):
        return round(1000 * int(cycles) / hz, 3)
    logical = dict(zip(("lecture_et_validation", "preparation_CoW", "publication_lots_F1_F2"),
                       map(ms, json.loads(phase)), strict=True))
    hardware = json.loads(block)
    assert len(hardware) == 6
    physical = {op: {"nombre": hardware[2*i], "millisecondes": ms(hardware[2*i + 1])}
                for i, op in enumerate(("lecture", "ecriture", "flush"))}
    submits = sum(item["nombre"] for item in physical.values())
    lots = re.findall(r"SAVE groupes=(\[[^\]]+\]) barrieres=(\[[^\]]+\])", text)
    phases = {"lots_64k_ms": logical}
    requetes = submits
    if lots:
        ((vecteur, marques),) = lots
        lectures, pages_lues, ecritures, pages_ecrites, avec_flush, pages_finales = json.loads(vecteur)
        dma = re.findall(r"SAVE dma=(\[[^\]]+\])", text)
        assert 0 <= lectures <= pages_lues
        assert 0 < ecritures <= pages_ecrites
        assert 0 < avec_flush <= pages_finales
        assert physical["flush"]["nombre"] == avec_flush == 2
        f1, apres_f1, f2, apres_f2 = json.loads(marques)
        assert 0 < f1 < apres_f1 <= f2 < apres_f2 <= int(fields["duree"])
        (appels,) = re.findall(r"SAVE appels_file=(\[[^\]]+\])", text)
        reserve, copie, position, soumet, termine = json.loads(appels)
        assert reserve == termine == lectures + ecritures + avec_flush
        if dma:
            (trace,) = dma
            (groupes_in, groupes_out, pages_in, pages_out, maximum, cadres, copie_in, copie_out,
             kicks, soumission, used, acquit, irq, traitement_irq, delai_irq) = json.loads(trace)
            assert pages_in == pages_lues and pages_out == pages_ecrites + pages_finales
            assert 0 <= groupes_in <= min(pages_in, physical["lecture"]["nombre"])
            assert 0 < groupes_out <= min(pages_out, physical["ecriture"]["nombre"])
            assert 1 <= maximum <= 64 and maximum <= cadres <= 128
            assert kicks == irq == submits
            assert copie == position == 0 and soumet == avec_flush
            requetes += lectures + ecritures - groupes_in - groupes_out
        else:
            assert pages_lues <= physical["lecture"]["nombre"]
            assert pages_ecrites + pages_finales <= physical["ecriture"]["nombre"]
            assert copie == position == pages_lues + pages_ecrites + pages_finales
            assert soumet == copie + avec_flush
            requetes += lectures + ecritures - pages_lues - pages_ecrites - pages_finales
        phases = dict(appels_file_locaux=dict(reserve=reserve, copie=copie, position=position,
                                              soumet=soumet, termine=termine), lots=json.loads(vecteur), preparation_et_donnees_ms=ms(f1),
                      metadonnees_et_F1_ms=ms(apres_f1-f1),
                      ancre_et_F2_ms=ms(apres_f2-f2), persistance_acquise_ms=ms(apres_f2))
        if dma:
            phases["dma"] = dict(groupes_in=groupes_in, groupes_out=groupes_out, pages_in=pages_in,
                                 pages_out=pages_out, max_pages_par_chaine=maximum, cadres_arene=cadres,
                                 kicks=kicks, irq=irq, copie_in_ms=ms(copie_in), copie_out_ms=ms(copie_out),
                                 soumission_ms=ms(soumission), kick_a_used_observe_ms=ms(used),
                                 used_a_acquittement_ms=ms(acquit), traitement_irq_ms=ms(traitement_irq),
                                 kick_a_entree_irq_ms=ms(delai_irq))
        temoin = re.findall(r"SAVE dma_4k=(\[[^\]]+\])", text)
        if temoin:
            (trace,) = temoin
            lectures_dma, ecritures_dma, *mesures = json.loads(trace)
            zero = mesures[:4]
            copie_in, copie_out, kicks, soumission, used, acquit, irq, traitement_irq, delai_irq = mesures[4:]
            assert not dma and zero == [0, 0, 0, 0] and kicks == irq == submits
            assert lectures_dma == physical["lecture"]["nombre"] and ecritures_dma == physical["ecriture"]["nombre"]
            phases["dma_4k"] = dict(kicks=kicks, irq=irq, copie_in_ms=ms(copie_in), copie_out_ms=ms(copie_out),
                                    soumission_ms=ms(soumission), kick_a_used_observe_ms=ms(used),
                                    used_a_acquittement_ms=ms(acquit), traitement_irq_ms=ms(traitement_irq),
                                    kick_a_entree_irq_ms=ms(delai_irq))
    moteur = re.findall(r"SAVE durees_moteur=(\[[^\]]+\])", text)
    if moteur:
        ticks = json.loads(moteur[0])
        noms = ("sha256_source", "sha256_candidate_et_extents", "validation_texte", "reservation_pages")
        if len(ticks) != 4:
            noms = ("sha256_source", "sha256_candidate", "validation_texte", "reservation_pages",
                    "construction_extents", "copie_source_et_pret", "allocation_fenetres",
                    "recherche_extents", "effacement_liberation")
        phases["phases_moteur_ms"] = dict(zip(noms, map(ms, ticks), strict=True))
    durees = re.findall(r"SAVE durees_lots=(\[[^\]]+\])", text)
    if durees:
        lecture, ecriture = map(ms, json.loads(durees[0]))
        phases.update(lots_bloc_lecture_ms=lecture, lots_bloc_ecriture_ms=ecriture)
    assert counts["moteur_bloc"] == counts["bloc_pilote"] == requetes
    gifts = int(fields["dons"])
    total = sum(counts.values()) + gifts
    elapsed = round(row["transfert_avant_oui_ms"] + row["publication_apres_oui_ms"], 3)
    prets = re.findall(r"SAVE pret_local=(\[[^\]]+\])", text)
    locaux_pret = json.loads(prets[0]) if prets else [0] * 6
    assert len(locaux_pret) == 6
    # Compter aussi les contrôles du prêt (sceller, déléguer, admettre, retirer,
    # libérer). Les copies synchrones internes sont ventilées comme les copies bloc.
    controles_pret = sum(locaux_pret) - locaux_pret[2]
    conservateur = total + sum(controls.values()) + controles_pret
    return dict(appels_locaux_pret=locaux_pret, avec_copies_locales_pret=conservateur + locaux_pret[2],
                ipc_par_liaison=counts, dons_acquittes=gifts, total=total,
                controles_noyau_locaux=controls, put=int(fields["put"]), take=int(fields["take"]), bloc=physical,
                total_avec_controles_locaux=conservateur, **phases, appel_publish_ms=ms(fields["duree"]),
                commande_au_succes_ms=elapsed, oui_au_succes_ms=row["publication_apres_oui_ms"],
                session_compteur_historique=int(re.search(r"STOCKAGE trafic tours=(\d+)", text)[1]),
                ipc_conforme=conservateur <= 64, temps_conforme=elapsed < 1000)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("journal", type=Path, help="trace de la VM avec io-0166-sauvegarde.patch")
    p.add_argument("mesure", type=Path, help="JSON de vi_performance.py après vérification des images")
    args = p.parse_args()
    result = bilan(args.journal, args.mesure)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    raise SystemExit(0 if result["ipc_conforme"] and result["temps_conforme"] else 1)
