#!/usr/bin/env python3
"""Trekk ut, kontroller og bygg inn gettext-katalogene til Agentic HW Monitor."""

import argparse
import ast
from dataclasses import dataclass, field
import json
from functools import cache
from pathlib import Path
import re
import sys
import tomllib

APP = Path(__file__).resolve().parents[1]
ROOT = APP / "data/i18n"
RULES = dict.fromkeys(
    ("nb", "en", "de", "es", "it", "sv"), "nplurals=2; plural=(n != 1);"
) | dict.fromkeys(
    ("fr", "pt-BR"), "nplurals=2; plural=(n > 1);"
) | dict.fromkeys(
    ("zh-CN", "ja"), "nplurals=1; plural=0;"
) | {
    "pl": "nplurals=3; plural=(n == 1 ? 0 : n % 10 >= 2 && n % 10 <= 4 && (n % 100 < 12 || n % 100 > 14) ? 1 : 2);"
}
# Samme rekkefølge som `Language::ALL` i rigg_core::i18n (nedtrekkslista).
LANGS = ("nb", "en", "de", "fr", "es", "pt-BR", "it", "pl", "sv", "zh-CN", "ja")
PLURAL_TOKEN = re.compile(r'\s*(\d+|n|\|\||&&|[=!<>]=|[()?:%<>])')
TOKEN = re.compile(
    r'//[^\n]*|/\*|(?P<raw>r(?P<hash>\#*)"(?s:.*?)"(?P=hash))'
    r'|(?P<string>"(?:[^"\\]|\\.)*")|\'(?:[^\'\\\n]|\\[^\n]+?)\''
    r'|[\w]+|[^\s]', re.S
)
DIRECTIVE = re.compile(r'(msgctxt|msgid_plural|msgid|msgstr(?:\[\d+\])?)\s+(".*")$')
PLACEHOLDER = re.compile(r'\{\{|\}\}|\{([A-Za-z_][A-Za-z_0-9]*)\}|[{}]')
BLOCK = re.compile(r'/\*|\*/')


class Invalid(ValueError):
    pass


@dataclass
class Entry:
    context: str = ""
    source: str = ""
    plural: str | None = None
    translations: dict[int, str] = field(default_factory=dict)
    comments: list[str] = field(default_factory=list)
    lines: list[int] = field(default_factory=list)
    formatted: bool = False

    @property
    def key(self):
        return self.context, self.source

    @property
    def fuzzy(self):
        return any("fuzzy" in line[2:].replace(",", " ").split()
                   for line in self.comments if line.startswith("#,"))


def rust_string(token):
    if token.startswith("r"):
        quote = token.index('"')
        return token[quote + 1:-quote]
    body = token[1:-1]
    escapes = {"n": "\n", "r": "\r", "t": "\t", "0": "\0", "\\": "\\", '"': '"', "'": "'"}
    def unescape(match):
        escape = match[1]
        if escape[0] == "u":
            return chr(int(escape[2:-1].replace("_", ""), 16))
        if escape[0] == "x":
            return chr(int(escape[1:], 16))
        if escape.startswith("\n"):
            return ""
        return escapes[escape]
    return re.sub(r'\\(u\{[\da-fA-F_]+\}|x[\da-fA-F]{2}|\n\s*|.)', unescape, body)


def tokens(text):
    end = 0
    while match := TOKEN.search(text, end):
        end = match.end()
        token = match[0]
        if token.startswith("//"):
            continue
        if token == "/*":
            depth = 1
            while depth and (block := BLOCK.search(text, end)):
                depth += 1 if block[0] == "/*" else -1
                end = block.end()
            if depth:
                raise Invalid("uavsluttet Rust-kommentar")
            continue
        yield token, match.start(), match.lastgroup in ("raw", "string")


def entries_from_source(path):
    text = path.read_text(encoding="utf-8")
    if not re.search(r'\btr[fn]?\b', text):
        return {}
    parts = list(tokens(text))
    entries = {}
    for i, (macro, pos, _) in enumerate(parts):
        if macro not in ("tr", "trf", "trn") or parts[i + 1][0] != "!":
            continue
        opening = {"(": ")", "[": "]", "{": "}"}
        line = text.count("\n", 0, pos) + 1
        index = i + 2
        stack = [opening[parts[index][0]]]
        index += 1
        if macro == "trn":
            while stack:
                token = parts[index][0]
                index += 1
                if token == "," and len(stack) == 1:
                    break
                if token in opening:
                    stack.append(opening[token])
                elif token == stack[-1]:
                    stack.pop()
        strings = []
        while parts[index][2]:
            strings.append(rust_string(parts[index][0]))
            index += 1
            if parts[index][0] != ",":
                break
            index += 1
        required = 2 if macro == "trn" else 1
        if len(strings) not in (required, required + 1):
            raise Invalid(f"{path}:{line}: oversettelsesnøkkelen må være en strengliteral")
        context = strings.pop(0) if len(strings) > required else ""
        entry = Entry(context, strings[0], strings[1] if required == 2 else None,
                      formatted=macro == "trf" or (macro == "trn" and parts[index][0] != stack[0]))
        if not entry.source:
            raise Invalid(f"{path}:{line}: tom msgid er reservert til PO-hodet")
        if entry.formatted:
            expected_names = placeholders(entry.source)
            if entry.plural is not None and placeholders(entry.plural) != expected_names:
                raise Invalid(f"{path}:{line}: entall og flertall må ha de samme plassholderne")
        previous = entries.setdefault(entry.key, entry)
        if previous.plural != entry.plural or previous.formatted != entry.formatted:
            raise Invalid(f"{path}:{line}: {entry.key!r} brukes med ulik form, legg til kontekst")
        previous.lines.append(line)
    return entries


def po_string(value):
    # Gettext og JSON har samme skrivemåte for disse kontrolltegnene.
    return json.dumps(value, ensure_ascii=False)


def read_po(path):
    records = {}
    comments = []
    fields = {}
    current = None

    def finish():
        if not fields:
            return
        if "msgid" not in fields:
            raise Invalid(f"{path}: oppføring uten msgid")
        entry = Entry(fields.get("msgctxt", ""), fields["msgid"],
                      fields.get("msgid_plural"), comments=comments.copy())
        for key, val in fields.items():
            if key.startswith("msgstr"):
                index = int(key[7:-1]) if key != "msgstr" else 0
                if index in entry.translations or (entry.plural is None) != (key == "msgstr"):
                    raise Invalid(f"{path}: feil msgstr for {entry.key!r}")
                entry.translations[index] = val
        if entry.key in records:
            raise Invalid(f"{path}: duplikat {entry.key!r}")
        records[entry.key] = entry
        comments.clear()
        fields.clear()

    for line, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        token = raw.strip()
        if not token:
            finish()
            current = None
        elif token.startswith("#~"):
            continue
        elif token.startswith("#"):
            finish()
            current = None
            if not token.startswith("#:"):
                comments.append(token)
        elif match := DIRECTIVE.fullmatch(token):
            current, value = match.groups()
            if current in fields or (current == "msgctxt" and "msgid" in fields):
                finish()
            fields[current] = ast.literal_eval(value)
        elif token.startswith('"') and current is not None:
            fields[current] += ast.literal_eval(token)
        else:
            raise Invalid(f"{path}:{line}: ugyldig PO-linje")
    finish()
    return records


def nplurals(lang):
    return int(RULES[lang][9])


def plural_form(lang, n):
    """Regner ut `plural=`-uttrykket i språkets Plural-Forms for antallet `n`."""
    expression = RULES[lang].partition("plural=")[2].rstrip(";")
    parts = PLURAL_TOKEN.findall(expression)
    if "".join(parts) != "".join(expression.split()):
        raise Invalid(f"{lang}: ukjent tegn i Plural-Forms")
    position = 0

    def peek():
        return parts[position] if position < len(parts) else None

    def take(expected=None):
        nonlocal position
        token = peek()
        if token is None or (expected is not None and token != expected):
            raise Invalid(f"{lang}: ugyldig Plural-Forms")
        position += 1
        return token

    def primary():
        token = take()
        if token == "(":
            value = ternary()
            take(")")
            return value
        if token == "n":
            return n
        if token.isdigit():
            return int(token)
        raise Invalid(f"{lang}: ugyldig Plural-Forms")

    def binary(operators, operand):
        value = operand()
        while peek() in operators:
            value = int(operators[take()](value, operand()))
        return value

    def modulo():
        return binary({"%": lambda a, b: a % b}, primary)

    def relation():
        return binary({"<": lambda a, b: a < b, ">": lambda a, b: a > b,
                       "<=": lambda a, b: a <= b, ">=": lambda a, b: a >= b}, modulo)

    def equality():
        return binary({"==": lambda a, b: a == b, "!=": lambda a, b: a != b}, relation)

    def conjunction():
        return binary({"&&": lambda a, b: bool(a) and bool(b)}, equality)

    def disjunction():
        return binary({"||": lambda a, b: bool(a) or bool(b)}, conjunction)

    def ternary():
        condition = disjunction()
        if peek() != "?":
            return condition
        take("?")
        yes = ternary()
        take(":")
        no = ternary()
        return yes if condition else no

    value = ternary()
    if position != len(parts) or not 0 <= value < nplurals(lang):
        raise Invalid(f"{lang}: Plural-Forms gir form {value} for n={n}")
    return value


def header(lang, previous=""):
    lines = previous.splitlines()
    new_fields = {
        "Project-Id-Version": "Agentic HW Monitor",
        "Language": lang.replace("-", "_"),
        "MIME-Version": "1.0",
        "Content-Type": "text/plain; charset=UTF-8",
        "Content-Transfer-Encoding": "8bit",
        "Plural-Forms": RULES[lang],
    }
    kept = [line for line in lines if line.partition(":")[0] not in new_fields]
    return "".join(f"{key}: {value}\n" for key, value in new_fields.items()) + "\n".join(kept) + ("\n" if kept else "")


def po_path(lang, module):
    return ROOT / lang / (module.replace("::", "/") + ".po")


def extract(module, path, entries):
    for lang in LANGS:
        dest = po_path(lang, module)
        previous = read_po(dest) if dest.exists() else {}
        head = previous.get(("", ""), Entry())
        meta = header(lang, head.translations.get(0, ""))
        lines = [*head.comments, 'msgid ""', 'msgstr ""',
                 *(po_string(line) for line in meta.splitlines(keepends=True))]
        for entry in entries.values():
            saved = previous.get(entry.key, Entry())
            strings = saved.translations if saved.plural == entry.plural else {}
            comments = saved.comments
            if saved.source and saved.plural != entry.plural and not saved.fuzzy:
                comments = [*comments, "#, fuzzy"]
            lines.extend(["", *comments,
                          "#: " + " ".join(f"{path.relative_to(APP)}:{n}" for n in entry.lines)])
            if entry.context:
                lines.append("msgctxt " + po_string(entry.context))
            lines.append("msgid " + po_string(entry.source))
            if entry.plural is None:
                translated = entry.source if lang == "nb" else strings.get(0, "")
                lines.append("msgstr " + po_string(translated))
            else:
                lines.append("msgid_plural " + po_string(entry.plural))
                for i in range(nplurals(lang)):
                    translated = (entry.source, entry.plural)[i] if lang == "nb" else strings.get(i, "")
                    lines.append(f"msgstr[{i}] " + po_string(translated))
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text("\n".join(lines) + "\n", encoding="utf-8")


def placeholders(text):
    names = set()
    for match in PLACEHOLDER.finditer(text):
        if match[1]:
            names.add(match[1])
        elif len(match[0]) == 1:
            raise Invalid(f"{text!r}: ugyldig plassholder, bruk navngitte felt eller doble klammer")
    return names


def check(lang, module, entries):
    path = po_path(lang, module)
    records = read_po(path) if path.exists() else {}
    head = records.pop(("", ""), Entry()).translations.get(0, "")
    complete = "X-Rigg-Status: complete" in head.splitlines()
    if path.exists():
        fields = dict(line.split(": ", 1) for line in head.splitlines() if ": " in line)
        if fields.get("Language") != lang.replace("-", "_") or fields.get("Plural-Forms") != RULES[lang]:
            raise Invalid(f"{path}: feil Language eller Plural-Forms, kjør uttrekket igjen")
    valid = {}
    for key, record in records.items():
        entry = entries.get(key)
        if entry is None:
            raise Invalid(f"{path}: foreldet nøkkel {key!r}")
        if record.plural != entry.plural:
            raise Invalid(f"{path}: feil msgid_plural for {key!r}")
        expected = nplurals(lang) if entry.plural is not None else 1
        if set(record.translations) != set(range(expected)):
            raise Invalid(f"{path}: forventet {expected} msgstr for {key!r}")
        forms = list(record.translations[i] for i in range(expected))
        if lang == "nb" and forms != ([entry.source] if entry.plural is None else [entry.source, entry.plural]):
            raise Invalid(f"{path}: bokmål skal gjengi kildeteksten for {key!r}")
        for i, form in enumerate(forms):
            if form and entry.formatted:
                source = entry.source if i == 0 or entry.plural is None else entry.plural
                expected_names = placeholders(source)
                if placeholders(form) != expected_names:
                    raise Invalid(f"{path}: plassholdere i {key!r}, form {i}, skal være {sorted(expected_names)}")
        if all(forms) and not record.fuzzy:
            valid[key] = forms
    missing = entries.keys() - valid.keys() if lang != "nb" or path.exists() else set()
    if complete and missing:
        raise Invalid(f"{path}: fullført katalog mangler " + ", ".join(repr(key) for key in sorted(missing)))
    return valid, missing


@cache
def crate_name(crate):
    meta = tomllib.loads((APP / "crates" / crate / "Cargo.toml").read_text(encoding="utf-8"))
    binary = meta.get("bin", [])
    primary = next((b["name"] for b in binary if b.get("path") == "src/main.rs"), meta["package"]["name"])
    return meta.get("lib", {}).get("name", primary).replace("-", "_")


def module_of(path):
    crate, src, *parts = path.relative_to(APP / "crates").with_suffix("").parts
    if src != "src":
        raise Invalid(f"{path}: ventet en kildefil under app/crates/<crate>/src")
    if parts[-1] == "mod" or parts in (["lib"], ["main"]):
        parts.pop()
    return "::".join([crate_name(crate), *parts])


def sources():
    return {module_of(path): path for path in sorted(APP.glob("crates/*/src/**/*.rs"))}


def catalog_modules():
    modules = set()
    for path in ROOT.glob("**/*.po"):
        lang, *parts = path.relative_to(ROOT).with_suffix("").parts
        if lang not in LANGS or not parts:
            raise Invalid(f"{path}: ukjent språk eller modul")
        modules.add("::".join(parts))
    return modules


def rust_quote(value):
    # Rust bruker \u{…}, ikke JSONs \uNNNN, for kontrolltegn.
    text = po_string(value)
    return re.sub(r'(\\\\)|\\([bf]|u[0-9a-f]{4})', lambda m: m[1] or {
        "b": r"\u{8}", "f": r"\u{c}",
    }.get(m[2], r"\u{" + m[2][1:] + "}"), text)


def build(dest, modules, index):
    by_lang = {lang: [] for lang in LANGS}
    for module in sorted(modules):
        path = index[module]
        entries = entries_from_source(path)
        print(f"cargo:rerun-if-changed={path}")
        for lang in LANGS:
            rows, _ = check(lang, module, entries)
            if lang != "nb":
                for (context, source), forms in rows.items():
                    words = ", ".join(rust_quote(s) for s in forms)
                    key = ", ".join(map(rust_quote, (module, context, source)))
                    by_lang[lang].append(f"            (({key}), &[{words}]),")
    lines = [
        "type CatalogRow = (MsgKey, Forms);",
        "fn catalog(lang: Language) -> &'static [CatalogRow] {",
        "    match lang.code() {",
    ]
    for lang in LANGS[1:]:
        lines.extend([f'        "{lang}" => &[', *by_lang[lang], "        ],"])
    lines.extend(["        _ => &[],", "    }", "}"])
    # Fasit for rigg_core::i18n::Language::plural_index, regnet ut fra
    # Plural-Forms i katalogene: (språk, antall former, form for n = 0..999).
    lines.extend(["#[cfg(test)]", "const PLURAL_TABLE: &[(&str, usize, &[u8])] = &["])
    for lang in LANGS:
        table = ", ".join(str(plural_form(lang, n)) for n in range(1000))
        lines.append(f'    ("{lang}", {nplurals(lang)}, &[{table}]),')
    lines.append("];")
    dest.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("module", nargs="?", type=Path, help="Rust-fil å trekke ut eller kontrollere")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--check", action="store_true", help="vis mangler og kontroller katalogene")
    mode.add_argument("--build", type=Path, help=argparse.SUPPRESS)
    args = parser.parse_args()
    index = sources()
    modules = catalog_modules()
    unknown = modules - index.keys()
    if unknown:
        raise Invalid("Kataloger uten kildefil: " + ", ".join(sorted(unknown)))
    if args.build:
        build(args.build, modules, index)
    elif args.check:
        selected = {module_of(args.module.resolve())} if args.module else index.keys()
        for module in sorted(selected):
            entries = entries_from_source(index[module])
            if not entries and module not in modules:
                continue
            for lang in LANGS:
                _, missing = check(lang, module, entries)
                print(f"{lang}/{module} · {len(entries) - len(missing)}/{len(entries)}")
                for context, source in sorted(missing):
                    print(f"  {context + ' · ' if context else ''}{source!r}")
    elif args.module:
        path = args.module.resolve()
        module = module_of(path)
        entries = entries_from_source(path)
        if not entries:
            raise Invalid(f"{path}: fant ingen oversettelsesnøkler")
        extract(module, path, entries)
    else:
        parser.error("oppgi en kildefil eller --check")


if __name__ == "__main__":
    try:
        main()
    except (ValueError, OSError, SyntaxError, KeyError, IndexError) as error:
        sys.exit(str(error))
