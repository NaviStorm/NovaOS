"""Construit le profil BDS PIO uniquement dans une copie isolée d'EDK II."""
import argparse
from pathlib import Path
import subprocess
from preparer_ovmf_reseau import PCI, REVISION, CHANGES, SLOT

BDS = "OvmfPkg/Library/PlatformBootManagerLib/"


def preparer(repo):
    def git(*args):
        return subprocess.check_output(["git", "-C", repo, *args])

    if git("rev-parse", "HEAD").decode().strip() != REVISION:
        raise ValueError("Révision EDK II inattendue")
    edits = {PCI + name: [pair] for name, pair in CHANGES.items()}
    edits[BDS + "BdsPlatform.c"] = [("  TryRunningQemuKernel ();", "  NovaPioBoot ();")]
    edits[BDS + "BdsPlatform.h"] = [("#define _PLATFORM_SPECIFIC_BDS_PLATFORM_H_",
                                      "#define _PLATFORM_SPECIFIC_BDS_PLATFORM_H_\n\nVOID NovaPioBoot (VOID);")]
    edits[BDS + "PlatformBootManagerLib.inf"] = [
        ("  BdsPlatform.c", "  BdsPlatform.c\n  NovaBootPio.c"),
        ("[Protocols]", "[Protocols]\n  gEfiRamDiskProtocolGuid\n  gEfiSimpleFileSystemProtocolGuid\n  gEfiBlockIoProtocolGuid"),
        ("[Guids]", "[Guids]\n  gEfiVirtualDiskGuid"),
    ]
    changes = {}
    for name, pairs in edits.items():
        raw = git("show", "HEAD:" + name)
        newline = "\r\n" if b"\r\n" in raw else "\n"
        target = raw.decode().replace("\r\n", "\n")
        for before, after in pairs:
            if target.count(before) != 1:
                raise ValueError("Ancre non unique dans " + name)
            target = target.replace(before, after)
        historical = target
        if name.endswith("PciEnumeratorSupport.c"):
            historical += SLOT
            # Tout endpoint PCI reste fermé durant BDS, y compris les fonctions intégrées.
            target += SLOT[:SLOT.index("  return (BOOLEAN)")] + "  return (BOOLEAN)(Device->Parent != NULL);\n}\n"
        def encode(s):
            return s.replace("\n", newline).encode()
        path = repo / name
        content = encode(target)
        if path.read_bytes() not in (raw, encode(historical), content):
            raise ValueError("Modifications locales imprévues dans " + name)
        changes[path] = content
    cfile = repo / BDS / "NovaBootPio.c"
    content = Path(__file__).with_name("NovaBootPio.c").read_bytes()
    if cfile.exists() and cfile.read_bytes() != content:
        raise ValueError("Source PIO locale divergente")
    changes[cfile] = content
    for path, content in changes.items():
        path.write_bytes(content)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("edk2", type=Path)
    preparer(parser.parse_args().edk2.resolve())
