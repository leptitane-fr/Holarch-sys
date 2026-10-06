#!/usr/bin/env python3
"""Le catalogue du dépôt pour la Bibliothèque de Holarch (le « Holons Store ») : chaque
holon qui a une vitrine (`vitrine.txt`) y figure, avec son icône, ses captures et sa
description. Recalculé à partir des fichiers, comme l'index.

La vitrine d'un holon, `<dossier>/<nom>/vitrine.txt`, un champ par ligne :
    nom = …            le nom affiché
    catégorie = …      voir CATEGORIES (« Atelier » pour les modes de l'Atelier)
    cible = …          « Holarch System », « Holarch pour Windows », ou les deux séparés par « , »
    résumé = …         une ligne
    version = …
    icône = icone.svg  (dans le dossier du holon ; SVG ou PNG carré)
    captures = captures/1.webp, captures/2.webp   (facultatif)
La description longue est dans `description.txt` (paragraphes séparés par une ligne vide).
La licence, l'auteur et l'état viennent de `ORIGINE.txt` (et, ici, de l'index).

Usage : python outils/catalogue.py            écrit catalogue.json
        python outils/catalogue.py --verifie  refuse si catalogue.json n'est pas à jour
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DOSSIERS = {"pilotes": "Holon-sys", "apps": "Holon-app"}
CATEGORIES = {
    "Holon-sys": ["Pilotes", "Système", "Réseau", "Sécurité"],
    "Holon-app": ["Atelier", "Productivité", "Création", "Multimédia", "Communication", "Internet", "Outils", "Éducation", "Jeux"],
}
CIBLES = {"Holarch System", "Holarch pour Windows"}


def champs(path):
    if not path.is_file():
        return {}
    return dict(l.split(" = ", 1) for l in path.read_text("utf-8").splitlines() if " = " in l)


def etats_index():
    """Holarch-sys : l'état de chaque pilote d'après l'index (rapports signés)."""
    etats = {}
    index = ROOT / "index.txt"
    if index.is_file():
        for l in index.read_text("utf-8").splitlines():
            if l.startswith("pilote ") and " : " in l:
                nom, etat = l[len("pilote "):].split(" : ", 1)
                etats.setdefault(nom, etat)
    return etats


def holon(dossier, genre, etats):
    v = champs(dossier / "vitrine.txt")
    o = champs(dossier / "ORIGINE.txt")
    rel = dossier.relative_to(ROOT).as_posix()
    erreurs = [f"{rel}/vitrine.txt : « {k} » manque" for k in ("nom", "catégorie", "cible", "résumé", "version", "icône") if k not in v]
    if erreurs:
        raise SystemExit("\n".join(erreurs))
    if v["catégorie"] not in CATEGORIES[genre]:
        raise SystemExit(f"{rel} : catégorie « {v['catégorie']} » inconnue pour un {genre} ({', '.join(CATEGORIES[genre])})")
    cibles = [c.strip() for c in v["cible"].split(",")]
    if not set(cibles) <= CIBLES:
        raise SystemExit(f"{rel} : cible inconnue ({', '.join(sorted(CIBLES))})")
    fichiers = [v["icône"]] + [c.strip() for c in v.get("captures", "").split(",") if c.strip()]
    for f in fichiers:
        if not (dossier / f).is_file():
            raise SystemExit(f"{rel}/{f} : fichier introuvable")
    desc = (dossier / "description.txt").read_text("utf-8").strip() if (dossier / "description.txt").is_file() else ""
    return {
        "id": dossier.name,
        "genre": genre,
        "nom": v["nom"],
        "categorie": v["catégorie"],
        "cibles": cibles,
        "resume": v["résumé"],
        "description": [p.replace("\n", " ") for p in desc.split("\n\n") if p.strip()],
        "version": v["version"],
        "licence": o.get("licence", "?"),
        "auteur": o.get("auteur", "?"),
        "etat": etats.get(dossier.name, o.get("état", "proposé")),
        "socle": v.get("socle") == "oui",
        "icone": f"{rel}/{v['icône']}",
        "captures": [f"{rel}/{c}" for c in fichiers[1:]],
        "source": rel,
    }


def build():
    etats = etats_index()
    holons = []
    for d, genre in DOSSIERS.items():
        if (ROOT / d).is_dir():
            for p in sorted((ROOT / d).iterdir()):
                if (p / "vitrine.txt").is_file():
                    holons.append(holon(p, genre, etats))
    cat = {"format": 1, "holons": holons}
    return json.dumps(cat, ensure_ascii=False, indent=1) + "\n"


def main():
    texte = build()
    sortie = ROOT / "catalogue.json"
    if "--verifie" in sys.argv:
        if not sortie.is_file() or sortie.read_text("utf-8") != texte:
            raise SystemExit("catalogue.json n'est pas à jour : python outils/catalogue.py")
        print("catalogue.json à jour")
        return
    sortie.write_text(texte, "utf-8")
    print(f"catalogue.json : {len(json.loads(texte)['holons'])} holons")


if __name__ == "__main__":
    main()
