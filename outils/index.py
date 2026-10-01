#!/usr/bin/env python3
"""L'index du dépôt, recalculé à partir des fichiers : demandes, pilotes et
rapports d'essai. Seuls les rapports signés par une clé de cles/ comptent :
l'index n'est pas une signature, chacun peut le régénérer et le comparer.

Usage : python outils/index.py           écrit index.txt
        python outils/index.py --verifie refuse si index.txt n'est pas à jour
                                         ou si un rapport est mal signé
"""
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import verifie  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent


def champs(path):
    return dict(l.split(" = ", 1) for l in path.read_text("utf-8").splitlines() if " = " in l)


def machines():
    text = (ROOT / "cles" / "attestation.txt").read_text("utf-8")
    return {v.strip(): k.strip() for k, v in (l.split("=", 1) for l in text.splitlines() if "=" in l and not l.startswith("#"))}


def rapport(path, known):
    text = path.read_text("utf-8")
    at = text.find("attestation = ")
    f = champs(path)
    ok = at >= 0 and verifie.verify(bytes.fromhex(f["attestation"]), text[:at].encode("utf-8"), bytes.fromhex(f["signature"]))
    if not ok:
        raise SystemExit(f"{path} : signature fausse")
    machine = known.get(f["attestation"])
    if machine is None:
        raise SystemExit(f"{path} : clé inconnue")
    return f, machine


def build():
    known = machines()
    lines = ["# Index de Holarch-sys, recalculé par outils/index.py (ne pas écrire à la main).", ""]
    for d in sorted((ROOT / "demandes").iterdir()):
        if d.is_dir():
            lines.append(f"demande {d.name} : {champs(d / 'demande.txt').get('état', '?')}")
    for p in sorted((ROOT / "pilotes").iterdir()):
        if not p.is_dir():
            continue
        essais = sorted((ROOT / "essais" / p.name).glob("*.txt")) if (ROOT / "essais" / p.name).is_dir() else []
        if not essais:
            lines.append(f"pilote {p.name} : proposé")
        for e in essais:
            f, machine = rapport(e, known)
            lines.append(f"pilote {p.name} : en essai, attesté par {machine} ({f['niveau']}, pannes {f['état'].split('pannes = ')[-1]}) : "
                         f"ELF {f['empreinte-elf'][:16]}…, Holarch {f['aiwos']}")
    return "\n".join(lines) + "\n"


def main():
    text = build()
    index = ROOT / "index.txt"
    if "--verifie" in sys.argv:
        if not index.exists() or index.read_text("utf-8") != text:
            raise SystemExit("index.txt n'est pas à jour : python outils/index.py")
        print("index.txt à jour ; rapports bien signés.")
    else:
        index.write_text(text, "utf-8", newline="\n")
        print(text, end="")


if __name__ == "__main__":
    main()
