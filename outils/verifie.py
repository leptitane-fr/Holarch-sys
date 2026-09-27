#!/usr/bin/env python3
"""Vérifie un rapport d'essai signé par Aiwos (essais/<pilote>/*.txt).

La signature Ed25519 porte sur tout ce qui précède la ligne
« attestation = » ; la clé doit être l'une de cles/attestation.txt.
Sans dépendance : Ed25519 d'après la RFC 8032, § 6 (vérification).

Usage : python outils/verifie.py essais/<pilote>/<fichier>.txt
"""
import hashlib
import sys
from pathlib import Path

p = 2**255 - 19
q = 2**252 + 27742317777372353535851937790883648493


def inv(x):
    return pow(x, p - 2, p)


d = -121665 * inv(121666) % p
sqrt_m1 = pow(2, (p - 1) // 4, p)


def add(P, Q):
    A, B = (P[1] - P[0]) * (Q[1] - Q[0]) % p, (P[1] + P[0]) * (Q[1] + Q[0]) % p
    C, D = 2 * P[3] * Q[3] * d % p, 2 * P[2] * Q[2] % p
    E, F, G, H = B - A, D - C, D + C, B + A
    return (E * F, G * H, F * G, E * H)


def mul(s, P):
    Q = (0, 1, 1, 0)
    while s > 0:
        if s & 1:
            Q = add(Q, P)
        P = add(P, P)
        s >>= 1
    return Q


def recover_x(y, sign):
    if y >= p:
        return None
    x2 = (y * y - 1) * inv(d * y * y + 1) % p
    if x2 == 0:
        return None if sign else 0
    x = pow(x2, (p + 3) // 8, p)
    if (x * x - x2) % p:
        x = x * sqrt_m1 % p
    if (x * x - x2) % p:
        return None
    return p - x if (x & 1) != sign else x


def decompress(b):
    y = int.from_bytes(b, "little")
    sign = y >> 255
    y &= (1 << 255) - 1
    x = recover_x(y, sign)
    return None if x is None else (x, y, 1, x * y % p)


def equal(P, Q):
    return (P[0] * Q[2] - Q[0] * P[2]) % p == 0 and (P[1] * Q[2] - Q[1] * P[2]) % p == 0


G = (recover_x(4 * inv(5) % p, 0), 4 * inv(5) % p, 1, recover_x(4 * inv(5) % p, 0) * (4 * inv(5) % p) % p)


def verify(public, msg, signature):
    A = decompress(public)
    R = decompress(signature[:32])
    s = int.from_bytes(signature[32:], "little")
    if A is None or R is None or s >= q:
        return False
    h = int.from_bytes(hashlib.sha512(signature[:32] + public + msg).digest(), "little") % q
    return equal(mul(s, G), add(R, mul(h, A)))


def main():
    path = Path(sys.argv[1])
    text = path.read_bytes().decode("utf-8")
    at = text.find("attestation = ")
    if at < 0:
        sys.exit("pas de ligne « attestation = »")
    body = text[:at].encode("utf-8")
    fields = dict(l.split(" = ", 1) for l in text[at:].splitlines() if " = " in l)
    public, signature = bytes.fromhex(fields["attestation"]), bytes.fromhex(fields["signature"])
    keys = Path(__file__).resolve().parent.parent / "cles" / "attestation.txt"
    known = {v.strip(): k.strip() for k, v in (l.split("=", 1) for l in keys.read_text("utf-8").splitlines() if "=" in l and not l.startswith("#"))}
    machine = known.get(fields["attestation"])
    if not verify(public, body, signature):
        sys.exit("SIGNATURE FAUSSE")
    if machine is None:
        sys.exit("signature juste, mais clé inconnue (absente de cles/attestation.txt)")
    print(f"Signature juste : rapport attesté par « {machine} ».")


if __name__ == "__main__":
    main()
