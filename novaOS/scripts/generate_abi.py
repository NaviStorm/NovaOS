"""Génère ou compare les interfaces natives NovaOS v1 (Python 3.11+)."""
import argparse
import os
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[1]
ABI = ROOT / "abi"
DOC = ROOT / "docs"
ORIGIN = "Généré par scripts/generate_abi.py depuis abi/syscalls.toml. Modifier les sources."
MODULES = ("native", "terminal", "block", "instances", "tmp")


def read_toml(path):
    with path.open("rb") as stream:
        return tomllib.load(stream)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def validate(registry, template):
    require((registry["schema_version"], registry["abi_version"], registry["word_bits"],
             registry["endianness"]) == (1, 1, 64, "little"), "format ABI inconnu")
    require(set(registry["calling"]) == {"x86_64", "aarch64"}, "architectures")
    for arch, gate in registry["calling"].items():
        require(gate["instruction"] and gate["selector"] and len(gate["arguments"]) == 3
                and len(gate["reply"]) == 2, f"convention de {arch}")
    require((ROOT / registry["reservation_source"]).is_file(), "référence des réservations")
    for low, high in registry["reserved"]:
        require(0x10000 <= low <= high <= 0x1ffff, "plage réservée invalide")
    calls = registry["call"]
    numbers = set()
    rust_names = set()
    constants = set(registry["statuses"])
    require(all(re.search(rf"pub const {name}: u64 = \d+;", template) for name in constants),
            "statuts absents du contrat natif")
    protocol = read_toml(ABI / "services.toml")["fichiers"]
    file_words = dict(re.findall(r"pub const (\w+): u64 = (\d+);", (ROOT / protocol["source"]).read_text()))
    for name in protocol["statuses"]:
        require(name in file_words and int(file_words[name]) <= 255, f"statut de fichier inconnu {name}")
        constants.add("files::" + name)
    require(registry["default_status"] == "active", "état par défaut inconnu")
    for name, spec in calls.items():
        require(spec.get("status", registry["default_status"]) in {"active", "deprecated"}, f"état de {name}")
        require(re.fullmatch(r"[A-Z][A-Z0-9_]*", name), f"nom invalide {name}")
        n = spec["number"]
        require(type(n) is int and n >> 16 == 1, f"numéro ABI v1 invalide pour {name}")
        require(n not in numbers, f"numéro dupliqué {n:#x}")
        require(all(not lo <= n <= hi for lo, hi in registry["reserved"]), f"numéro réservé {n:#x}")
        numbers.add(n)
        require(spec["module"] in MODULES, f"module de {name}")
        rust = spec.get("rust_name", name)
        require(re.fullmatch(r"[A-Z][A-Z0-9_]*", rust), f"constante Rust invalide {rust}")
        symbol = (spec["module"], rust)
        require(symbol not in rust_names, f"constante Rust dupliquée {symbol}")
        rust_names.add(symbol)
        for key in ("group", "since", "arguments", "result", "rights", "handler"):
            require(isinstance(spec[key], str) and spec[key].strip(), f"{name}.{key}")
        require(isinstance(spec["errors"], list) and set(spec["errors"]) <= constants,
                f"statuts inconnus pour {name}")
    return dict(sorted(calls.items(), key=lambda item: item[1]["number"]))


def native_source(template, calls):
    slots = re.findall(r"@(\w+)(:grouped)?@", template)
    native = {spec.get("rust_name", name): spec for name, spec in calls.items() if spec["module"] == "native"}
    require(len(slots) == len(native) and {k for k, _ in slots} == set(native),
            "désaccord entre le gabarit et les appels du module native")
    def replace(match):
        number = native[match[1]]["number"]
        if match[2]:
            return f"0x{number >> 16:04x}_{number & 65535:04x}"
        return hex(number)
    return re.sub(r"@(\w+)(:grouped)?@", replace, template)


def header(calls, template, layouts):
    out = [f"/* {ORIGIN} */", "#ifndef NOVA_ABI_V1_H", "#define NOVA_ABI_V1_H",
           "#include <stdint.h>", "#include <stddef.h>", "#ifdef __cplusplus",
           "#define NOVA_ABI_ASSERT static_assert", "#define NOVA_ABI_ALIGN alignof", "#else",
           "#define NOVA_ABI_ASSERT _Static_assert", "#define NOVA_ABI_ALIGN _Alignof", "#endif", ""]
    for name, spec in calls.items():
        literal = f"0x{spec['number']:X}" if spec["module"] == "tmp" else f"UINT64_C({spec['number']:#x})"
        out.append(f"#define NOVA_SYS_{name} {literal}")
    for name, width, value in re.findall(r"pub const (\w+): u(32|64) = (\d+);", template):
        out.append(f"#define NOVA_{name} UINT{width}_C({value})")
    for name, body in re.findall(r"pub struct (\w+) \{\n(.*?)\n\}", template, re.S):
        out.extend(["", f"typedef struct nova_{name} {{"])
        for field in body.splitlines():
            match = re.fullmatch(r"\s*pub (\w+): (?:u(32|64)|\[u8; (\d+)\]),", field)
            require(match, f"champ non représenté en C : {name}.{field.strip()}")
            member, width, count = match.groups()
            out.append(f"    uint{width}_t {member};" if width else f"    uint8_t {member}[{count}];")
        out.append(f"}} nova_{name};")
        size, align = layouts[name]
        out.append(f'NOVA_ABI_ASSERT(sizeof(nova_{name}) == {size}, "{name} size");')
        out.append(f'NOVA_ABI_ASSERT(NOVA_ABI_ALIGN(nova_{name}) == {align}, "{name} alignment");')
    for struct, field, offset in re.findall(r"offset_of!\((\w+), (\w+)\) == (\d+)", template):
        out.append(f'NOVA_ABI_ASSERT(offsetof(nova_{struct}, {field}) == {offset}, "{struct}.{field}");')
    out.extend(["#undef NOVA_ABI_ASSERT", "#undef NOVA_ABI_ALIGN", "#endif"])
    return "\n".join(out) + "\n"


def link(ref, label=None, scope=None):
    path, _, symbol = ref.partition("#")
    source = ROOT / path
    require(source.is_file(), f"référence absente : {ref}")
    dest = os.path.relpath(source, DOC)
    if symbol:
        content = source.read_text()
        body = content
        if scope:
            marker = re.search(r"^impl " + re.escape(scope) + r"\b", content, re.M)
            require(marker, f"impl absente : {scope}")
            body = content[marker.start():].split("\n}", 1)[0]
        match = re.search(r"\bfn\s+" + re.escape(symbol) + r"\b", body)
        require(match, f"fonction absente : {ref}")
        dest += "#L" + str(content[:content.index(body) + match.start()].count("\n") + 1)
    return f"[{label or symbol or source.name}]({dest})"


def sdk_link(api, file=None):
    owner, method = api.split("::")
    if file is None:
        file = {"Endpoint": "ipc", "JoinHandle": "thread", "memory": "lib",
                "clock": "lib"}.get(owner, owner)
    return link(f"libs/nova-system/src/{file}.rs#{method}", api, owner if owner[0].isupper() else None)


def document(registry, calls):
    out = ["<!-- " + ORIGIN + " -->", "# Index de l’ABI native v1", "",
           "Les numéros hexadécimaux identifient les appels sur les deux architectures. "
           "Les trois arguments et les deux résultats occupent chacun un mot de 64 bits, "
           "en little-endian. Le premier résultat porte le statut. Le second vaut zéro en cas d’erreur.",
           "", "| Architecture | Instruction | Numéro | Arguments | Statut, valeur |",
           "|---|---|---|---|---|"]
    for arch, g in registry["calling"].items():
        out.append(f"| {arch} | {g['instruction']} | {g['selector']} | {', '.join(g['arguments'])} | {', '.join(g['reply'])} |")
    out.extend(["", "Les noms qualifiés correspondent aux constantes Rust. Le lien noyau mène "
                "au traitement de l’appel. Les statuts indiqués couvrent les cas caractéristiques. "
                "Le traitement lié précise également les erreurs propagées. La colonne SDK indique "
                "une fonction de haut niveau lorsqu’elle existe.", "",
                "| Numéro | Constante | Groupe / depuis / état | Arguments → valeur | Autorité | Erreurs | Noyau | SDK Rust |",
                "|---|---|---|---|---|---|---|---|"])
    for name, s in calls.items():
        api = ", ".join(sdk_link(item) for item in s.get("api", "").split()) or "—"
        out.append(f"| {s['number']:#x} | {s['module']}::{s.get('rust_name', name)} | {s['group']} / {s['since']} / {s.get('status', registry['default_status'])} | "
                   f"{s['arguments']} → {s['result']} | {s['rights']} | {', '.join(s['errors']) or '—'} | "
                   f"{link(s['handler'])} | {api} |")
    out.extend(["", "Les pointeurs sont des adresses virtuelles de l’appelant, transmises sur 64 bits. "
                "Sur x86_64, SYSCALL détruit RCX et R11. Sur AArch64, x2 à x30 et SP_EL0 sont préservés. "
                + link("docs/abi.md", "Contrat des registres") + "."])
    reserved = ", ".join(f"{a:#x}…{b:#x}" for a, b in registry["reserved"])
    out.extend(["", f"Plages préservées par le registre : {reserved}. "
                + link(registry["reservation_source"], "Historique des sondes") + ".",
                "", "## Protocoles en espace utilisateur", "",
                "Le numéro de message est local à son protocole. Le service fichier NovaFS "
                "traite les requêtes du SDK via IPC. Le pilote bloc utilise son propre format. "
                "Le canal d’édition relie vi à nsh. L’admission des instances utilise une page "
                "privée et la porte noyau WATCH. " + link("user/instance_manager.rs#run", "Gestionnaire") + ".",
                "", "Le courtier tmp sérialise les opérations internes CREATE, REMOVE et IO dans une enveloppe "
                "de 168 octets. IO transporte une trame fichier de 128 octets. Un CALL transmet le statut "
                "non nul renvoyé par le service. Une interruption après remise peut renvoyer files::UNCERTAIN. "
                + link("docs/contrat-tmp-prive-v0.21.0.md", "Portée et révocation de la capacité") + ".",
                "", "| Protocole | Opcode | Opération | Trame | Traitement ou transport | SDK Rust |",
                "|---|---|---|---|---|---|"])
    for service, protocol in read_toml(ABI / "services.toml").items():
        path = protocol["source"]
        width = protocol.get("wire_type", "u8")
        require(width in {"u8", "u64"}, f"largeur d’opcode de {service}")
        values = dict(re.findall(rf"pub const (\w+): {width} = (\d+);", (ROOT / path).read_text()))
        numbers = set()
        for handler, names in protocol["groups"]:
            dest = link(handler)
            for name in names.split():
                number = int(values[name])
                require(number not in numbers, f"opcode dupliqué : {service}.{name}")
                numbers.add(number)
                api = protocol.get("api", {}).get(name)
                out.append(f"| {service} | {number} | {name} | {link(path)} | {dest} | "
                           f"{', '.join(sdk_link(item, 'files') for item in api.split()) if api else '—'} |")
    out.extend(["", "## Utiliser le registre", "",
                "Depuis la racine du dépôt, exécuter `make -C novaOS abi-generate` après édition de "
                + link("abi/syscalls.toml", "syscalls.toml")
                + ". La cible `make -C novaOS abi-check` compare les produits sans les écrire. "
                "Le noyau, la bibliothèque utilisateur et nova-system importent les constantes produites. "
                "L’en-tête " + "[C11 / C++11](../abi/generated/nova_abi.h)"
                + " expose constantes et structures. Le SDK Rust autonome livre le fichier native.rs "
                "généré avec la présentation de v0.2.0 pour conserver son empreinte.",
                "", "Les structures et les statuts figurent dans " + link("abi/native.rs.in", "le contrat de disposition")
                + ". Les 32 octets d’une identité durable sont décrits par "
                + link("abi/instances.rs#encode", "Identity::encode") + "."])
    return "\n".join(out) + "\n"


def outputs():
    registry = read_toml(ABI / "syscalls.toml")
    template = (ROOT / registry["layout_source"]).read_text()
    require(registry["status_source"] == registry["layout_source"], "source de statuts inconnue")
    calls = validate(registry, template)
    files = {ABI / "native.rs": native_source(template, calls),
             ABI / "generated/nova_abi.h": header(calls, template, registry["layout"]),
             DOC / "registre-abi.md": document(registry, calls)}
    for module in MODULES[1:]:
        lines = [f"// {ORIGIN}"]
        for name, spec in calls.items():
            if spec["module"] == module:
                lines.append(f"pub const {spec.get('rust_name', name)}: u64 = {spec['number']:#x};")
        files[ABI / f"generated/{module}.rs"] = "\n".join(lines) + "\n"
    return files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    try:
        for path, content in outputs().items():
            data = content.encode()
            if path.exists() and path.read_bytes() == data:
                continue
            if args.check:
                raise ValueError(f"régénération requise : {os.path.relpath(path, ROOT)}")
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
    except (ValueError, KeyError, OSError, TypeError) as error:
        parser.exit(1, f"Registre ABI : {error}\n")


if __name__ == "__main__":
    main()
