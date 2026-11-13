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
        assert 0 < lectures <= pages_lues <= physical["lecture"]["nombre"]
        assert 0 < ecritures <= pages_ecrites
        assert 0 < avec_flush <= pages_finales
        assert pages_ecrites + pages_finales <= physical["ecriture"]["nombre"]
        assert physical["flush"]["nombre"] == avec_flush == 2
        requetes += lectures + ecritures - pages_lues - pages_ecrites - pages_finales
        f1, apres_f1, f2, apres_f2 = json.loads(marques)
        assert 0 < f1 < apres_f1 <= f2 < apres_f2 <= int(fields["duree"])
        (appels,) = re.findall(r"SAVE appels_file=(\[[^\]]+\])", text)
        reserve, copie, position, soumet, termine = json.loads(appels)
        assert reserve == termine == lectures + ecritures + avec_flush
        assert copie == position == pages_lues + pages_ecrites + pages_finales
        assert soumet == copie + avec_flush
        phases = dict(appels_file_locaux=dict(reserve=reserve, copie=copie, position=position,
                                              soumet=soumet, termine=termine), lots=json.loads(vecteur), preparation_et_donnees_ms=ms(f1),
                      metadonnees_et_F1_ms=ms(apres_f1-f1),
                      ancre_et_F2_ms=ms(apres_f2-f2), persistance_acquise_ms=ms(apres_f2))
    assert counts["moteur_bloc"] == counts["bloc_pilote"] == requetes
    gifts = int(fields["dons"])
    total = sum(counts.values()) + gifts
    elapsed = round(row["transfert_avant_oui_ms"] + row["publication_apres_oui_ms"], 3)
    return dict(ipc_par_liaison=counts, dons_acquittes=gifts, total=total,
                controles_noyau_locaux=controls, put=int(fields["put"]), take=int(fields["take"]), bloc=physical,
                total_avec_controles_locaux=total + sum(controls.values()), **phases, appel_publish_ms=ms(fields["duree"]),
                commande_au_succes_ms=elapsed, oui_au_succes_ms=row["publication_apres_oui_ms"],
                session_compteur_historique=int(re.search(r"STOCKAGE trafic tours=(\d+)", text)[1]),
                ipc_conforme=total + sum(controls.values()) <= 64, temps_conforme=elapsed < 1000)


if __name__ == "__main__":
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("journal", type=Path, help="trace de la VM avec io-0166-sauvegarde.patch")
    p.add_argument("mesure", type=Path, help="JSON de vi_performance.py après vérification des images")
    args = p.parse_args()
    result = bilan(args.journal, args.mesure)
    print(json.dumps(result, ensure_ascii=False, indent=2))
    raise SystemExit(0 if result["ipc_conforme"] and result["temps_conforme"] else 1)
