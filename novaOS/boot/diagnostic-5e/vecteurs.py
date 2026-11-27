"""Manifestes fictifs, signés avec la graine de test publique du catalogue 4B."""

from pathlib import Path
from struct import pack
import tomllib
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey

base = Path(__file__).resolve().parent
v = base.parents[1] / "libs/catalogue-pilotes/vecteurs"
reference = tomllib.loads((v / "pilote.toml").read_text())
image = next(i for i in reference["image"] if i["isa"] == "x86_64")
out = base / "vecteurs"
out.mkdir(exist_ok=True)
for short, device in (("ioapic", "ioapic"), ("hpet-a", "hpet"), ("hpet-b", "hpet")):
    raw = f'''schema="novaos.driver/1"
id="org.novaos.lab.{short}"
version=[1,0,0]
security_epoch=3
origin="laboratoire"
publisher="org.novaos.lab"
signing_key_id="lab-v1"
elf_package_id="{reference["elf_package_id"]}"
elf_package_revision=1
class="diagnostic"
provides=["diagnostic/1.0"]
required_caps=[]
optional_caps=[]
dependency=[]

[compat]
architectures=["x86_64"]
abi_major=1
os_min=[0,20,0]
os_before=[0,21,0]
required_features=[]

[[image]]
isa="x86_64"
member="pilote"
package_sha256="{image["package_sha256"]}"
elf_sha256="{image["elf_sha256"]}"

[[match]]
bus="platform"
provider="acpi"
firmware_id="{device}"

[hardware]
dma=[]
mmio=[]
irq=[]
max_irq_routes=0
dma_pages=0
queues=0
entries_per_queue=0

[quotas]
private_pages=0
ipc_roots_created=0
ipc_handles=0
ipc_boxes_created=0
pending_operations=0
'''.encode()
    key = Ed25519PrivateKey.from_private_bytes(bytes([1]) * 32)
    (out / (short + ".toml")).write_bytes(raw)
    (out / (short + ".sig")).write_bytes(
        key.sign(b"NovaOS/driver-package/v1\0" + pack("<I", len(raw)) + raw)
    )
