#!/usr/bin/env python3
"""Retrouve la fonction qui contient une adresse, dans l'ELF d'un programme
d'Aiwos : de quoi lire un rapport de panne (« pannes <n> ») sans addr2line
ni nm.

Les programmes d'Aiwos sont liés à une adresse fixe (0x400000, voir
programs/user.ld) : l'adresse d'une instruction dans Aiwos est son adresse
dans l'ELF. Le rapport donne l'instruction en cause et les adresses de
retour trouvées dans la pile ; ce script les nomme, grâce à la table des
symboles (gardée par la compilation en mode release).

Usage :
    python symboles.py <elf> <adresse>… [--appels <adresse>…]
    python symboles.py <elf>                 (toutes les fonctions, par adresse)

Les adresses après --appels sont des adresses de retour (les « appels
probables » d'un rapport) : l'instruction qui suit un appel. Elles sont
cherchées moins 1, dans l'appel lui-même : un appel qui ne revient pas
(une panique) est souvent la dernière instruction de sa fonction, et son
adresse de retour tombe au début de la suivante.

Aucune bibliothèque à installer : Python 3.8 ou plus suffit.
"""

import re
import struct
import sys

STT_FUNC = 2


def symbols(path):
    """Les fonctions de l'ELF (adresse, taille, nom), par adresse."""
    with open(path, "rb") as f:
        data = f.read()
    if data[:4] != b"\x7fELF" or data[4] != 2 or data[5] != 1:
        raise SystemExit(f"{path} : pas un ELF 64 bits petit-boutiste.")
    shoff, = struct.unpack_from("<Q", data, 0x28)
    shentsize, shnum = struct.unpack_from("<HH", data, 0x3A)
    sections = [struct.unpack_from("<IIQQQQIIQQ", data, shoff + i * shentsize) for i in range(shnum)]
    found = []
    for _, kind, _, _, offset, size, link, _, _, entsize in sections:
        if kind != 2:  # SHT_SYMTAB
            continue
        strings = sections[link]
        base = strings[4]
        for at in range(offset, offset + size, entsize or 24):
            name, info, _, _, value, length = struct.unpack_from("<IBBHQQ", data, at)
            if info & 0xF == STT_FUNC and value:
                end = data.index(b"\0", base + name)
                found.append((value, length, data[base + name : end].decode("utf-8", "replace")))
    if not found:
        raise SystemExit(f"{path} : pas de table des symboles (compilé avec strip ?).")
    return sorted(found)


BASIC = {
    "a": "i8", "b": "bool", "c": "char", "d": "f64", "e": "str", "f": "f32", "h": "u8", "i": "isize",
    "j": "usize", "l": "i32", "m": "u32", "n": "i128", "o": "u128", "s": "i16", "t": "u16", "u": "()",
    "v": "...", "x": "i64", "y": "u64", "z": "!", "p": "_",
}


class V0:
    """Les noms Rust de la forme « v0 » (« _R… », celle de Rust 2024) : le
    chemin en clair, sans les empreintes des crates."""

    def __init__(self, text):
        self.s, self.i, self.depth = text, 0, 0

    def peek(self):
        return self.s[self.i] if self.i < len(self.s) else ""

    def take(self, char=None):
        c = self.peek()
        if not c or (char and c != char):
            raise ValueError
        self.i += 1
        return c

    def base62(self):
        if self.peek() == "_":
            self.i += 1
            return 0
        n = 0
        while self.peek() != "_":
            c = self.take()
            n = n * 62 + "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ".index(c)
        self.i += 1
        return n + 1

    def disambiguator(self):
        if self.peek() == "s":
            self.i += 1
            self.base62()

    def ident(self):
        self.disambiguator()
        if self.peek() == "u":
            self.i += 1
        digits = re.match(r"\d+", self.s[self.i :])
        if not digits:
            raise ValueError
        self.i += digits.end()
        if self.peek() == "_":
            self.i += 1
        n = int(digits.group())
        name = self.s[self.i : self.i + n]
        self.i += n
        return name

    def backref(self, parse):
        target = self.base62()
        if self.depth > 50:
            raise ValueError
        saved = self.i
        self.i, self.depth = target, self.depth + 1
        result = parse()
        self.i, self.depth = saved, self.depth - 1
        return result

    def path(self):
        c = self.take()
        if c == "C":
            return self.ident()
        if c == "N":
            namespace = self.take()
            parent = self.path()
            name = self.ident()
            if namespace == "C":
                return f"{parent}::{{closure}}"
            return f"{parent}::{name}" if name else parent
        if c == "M":
            self.disambiguator()
            self.path()
            return f"<{self.type()}>"
        if c == "X":
            self.disambiguator()
            self.path()
            of = self.type()
            return f"<{of} as {self.path()}>"
        if c == "Y":
            of = self.type()
            return f"<{of} as {self.path()}>"
        if c == "I":
            base = self.path()
            args = []
            while self.peek() != "E":
                args.append(self.generic())
            self.i += 1
            return f"{base}<{', '.join(args)}>"
        if c == "B":
            return self.backref(self.path)
        raise ValueError

    def generic(self):
        if self.peek() == "L":
            self.i += 1
            self.base62()
            return "'_"
        if self.peek() == "K":
            self.i += 1
            return self.const()
        return self.type()

    def const(self):
        if self.peek() == "p":
            self.i += 1
            return "_"
        if self.peek() == "B":
            self.i += 1
            return self.backref(self.const)
        self.type()
        negative = self.peek() == "n"
        if negative:
            self.i += 1
        digits = re.match(r"[0-9a-f]*", self.s[self.i :]).group()
        self.i += len(digits)
        self.take("_")
        return f"{'-' if negative else ''}{int(digits or '0', 16)}"

    def type(self):
        c = self.peek()
        if c in BASIC:
            self.i += 1
            return BASIC[c]
        if c in "RQ":
            self.i += 1
            if self.peek() == "L":
                self.i += 1
                self.base62()
            return f"&{'mut ' if c == 'Q' else ''}{self.type()}"
        if c in "PO":
            self.i += 1
            return f"*{'mut' if c == 'O' else 'const'} {self.type()}"
        if c == "A":
            self.i += 1
            of = self.type()
            return f"[{of}; {self.const()}]"
        if c == "S":
            self.i += 1
            return f"[{self.type()}]"
        if c == "T":
            self.i += 1
            items = []
            while self.peek() != "E":
                items.append(self.type())
            self.i += 1
            return f"({', '.join(items)})"
        if c == "F":
            self.i += 1
            if self.peek() == "G":
                self.i += 1
                self.base62()
            if self.peek() == "U":
                self.i += 1
            if self.peek() == "K":
                self.i += 1
                if self.peek() == "C":
                    self.i += 1
                else:
                    self.ident()
            items = []
            while self.peek() != "E":
                items.append(self.type())
            self.i += 1
            return f"fn({', '.join(items)}) -> {self.type()}"
        if c == "D":
            self.i += 1
            if self.peek() == "G":
                self.i += 1
                self.base62()
            traits = []
            while self.peek() != "E":
                traits.append(self.path())
                while self.peek() == "p":
                    self.i += 1
                    self.ident()
                    self.type()
            self.i += 1
            self.generic()
            return f"dyn {' + '.join(traits)}"
        if c == "B":
            self.i += 1
            return self.backref(self.type)
        return self.path()


def demangle(name):
    """Un nom Rust en clair : forme « v0 » (« _R… ») ou ancienne
    (« _ZN…E »), sans les empreintes."""
    if name.startswith("_R"):
        parser = V0(name[2:])
        try:
            if parser.peek().isdigit():
                parser.i += 1
            return parser.path()
        except (ValueError, IndexError):
            return name
    match = re.fullmatch(r"_?_ZN(.*)E", name)
    if not match:
        return name
    rest, parts = match.group(1), []
    while rest:
        digits = re.match(r"\d+", rest)
        if not digits:
            return name
        n = int(digits.group())
        start = digits.end()
        parts.append(rest[start : start + n])
        rest = rest[start + n :]
    if parts and re.fullmatch(r"h[0-9a-f]{16}", parts[-1]):
        parts.pop()
    text = "::".join(parts)
    for code, char in (("$LT$", "<"), ("$GT$", ">"), ("$RF$", "&"), ("$BP$", "*"), ("$u20$", " "),
                       ("$u27$", "'"), ("$u5b$", "["), ("$u5d$", "]"), ("$u7b$", "{"), ("$u7d$", "}"),
                       ("$C$", ","), ("..", "::")):
        text = text.replace(code, char)
    return text


def main():
    if len(sys.argv) < 2:
        raise SystemExit(__doc__)
    table = symbols(sys.argv[1])
    if len(sys.argv) == 2:
        for value, length, name in table:
            print(f"{value:#x}\t{length}\t{demangle(name)}")
        return
    returns = False
    for text in sys.argv[2:]:
        if text == "--appels":
            returns = True
            print("Appels, du plus récent au plus ancien :")
            continue
        address = int(text, 16) if text.lower().startswith("0x") else int(text, 0)
        # Une adresse de retour : l'appel est juste avant.
        wanted = address - 1 if returns else address
        hits = [s for s in table if s[0] <= wanted < s[0] + max(s[1], 1)]
        if hits:
            value, _, name = hits[-1]
            where = "appel dans " if returns else ""
            print(f"{address:#x}  {where}{demangle(name)} +{address - value:#x}")
        else:
            print(f"{address:#x}  (aucune fonction : hors du code du programme ?)")


if __name__ == "__main__":
    main()
