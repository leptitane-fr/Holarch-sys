#!/usr/bin/env python3
"""Relais des demandes de pilotes acceptées à l'écran d'Aiwos (docs/11, D2).

Lit chaque fiche par le pont local (« demande <clé> texte »), vérifie que son
SHA-256 commence par l'empreinte montrée par Aiwos, et l'écrit dans
demandes/<dossier>/fiche.txt, à l'octet près. Écrit aussi demande.txt.
Rien n'est poussé : relire, puis git commit / git push.

Usage : outils/relaie_demandes.py <accord> <machine> <clé>=<empreinte> …
  ex. : outils/relaie_demandes.py 1 "Dell Latitude 7490" pci:00:1f.6=e6a1f4f33ac7b271
"""
import datetime
import hashlib
import json
import os
import re
import socket
import sys

PORT = int(os.environ.get("AIWOS_PORT", "7777"))
CONTROL_PORT = PORT + 1
TOKEN_FILE = os.path.join(os.path.expanduser("~"), f".aiwos-pont-{PORT}.jeton")
DEPOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def commande(texte):
    with open(TOKEN_FILE, encoding="ascii") as f:
        token = f.read().strip()
    with socket.create_connection(("127.0.0.1", CONTROL_PORT), timeout=200) as s:
        s.sendall(json.dumps({"jeton": token, "demande": "commande", "arg": texte, "maître": False}).encode() + b"\n")
        answer = json.loads(s.makefile("rb").readline().decode("utf-8"))
    if not answer["ok"]:
        raise RuntimeError(answer["texte"])
    return answer["texte"]


def dossier(cle, fiche):
    """La clé stable (docs/11) : pci-vvvv-dddd, usb-vvvv-pppp, acpi-<_HID ou _CID>."""
    if cle.startswith("pci:"):
        m = re.search(r"· ([0-9a-f]{4}):([0-9a-f]{4}) ·", fiche)
        return f"pci-{m[1]}-{m[2]}"
    if cle.startswith("usb:"):
        m = re.search(r"\b([0-9a-f]{4}):([0-9a-f]{4})\b", fiche)
        return f"usb-{m[1]}-{m[2]}"
    m = re.search(r"_HID[^\n]*?\b([A-Z]{3,4}[0-9A-F]{4})\b", fiche) or re.search(r"\b(PNP[0-9A-F]{4}|[A-Z]{3,4}[0-9A-F]{4})\b", fiche)
    return f"acpi-{m[1]}" if m else "acpi-" + cle.split(".")[-1]


def main():
    if len(sys.argv) < 4:
        sys.exit(__doc__)
    accord, machine = sys.argv[1], sys.argv[2]
    jour = datetime.date.today().isoformat()
    for arg in sys.argv[3:]:
        cle, empreinte = arg.rsplit("=", 1)
        fiche = commande(f"demande {cle} texte")
        if not fiche.endswith("\n"):
            fiche += "\n"
        octets = fiche.encode("utf-8")
        h = hashlib.sha256(octets).hexdigest()
        if not h.startswith(empreinte):
            print(f"{cle} : EMPREINTE DIFFÉRENTE ({h[:16]} au lieu de {empreinte}) : rien d'écrit")
            continue
        d = os.path.join(DEPOT, "demandes", dossier(cle, fiche))
        os.makedirs(d, exist_ok=True)
        with open(os.path.join(d, "fiche.txt"), "wb") as f:
            f.write(octets)
        titre = next((l for l in fiche.splitlines()[7:] if l.strip()), cle)
        with open(os.path.join(d, "demande.txt"), "w", encoding="utf-8", newline="\n") as f:
            f.write(f"appareil = {titre}\nmachine = {machine}\nétat = demandé\n"
                    f"demandé le = {jour}, accord {accord} à l'écran d'Aiwos (empreinte de la fiche {empreinte}…)\n"
                    f"voulu = étape 1 (lecture seule), puis à préciser par l'auteur du pilote\n")
        print(f"{cle} : {os.path.relpath(d, DEPOT)} ({len(octets)} octets, {h[:16]})")


if __name__ == "__main__":
    main()
