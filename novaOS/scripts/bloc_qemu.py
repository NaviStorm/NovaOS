"""Topologies QEMU validées. Image fichier uniquement ; lancement manuel en lecture seule."""
import argparse
import os
from pathlib import Path
import stat
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def command(arch, sector, image, *, readonly=False, debug=None, controls=()):
    image = image.resolve(strict=True)
    if not stat.S_ISREG(image.stat().st_mode) or any(c in str(image) for c in ':,'):
        raise ValueError("Image fichier attendue, sans virgule ni deux-points.")
    if sector not in (512, 4096) or arch not in ('x86_64', 'aarch64'):
        raise ValueError("Géométrie ou architecture non qualifiée.")
    arm = arch == 'aarch64'
    topology = (['-machine', 'msi=off', '-device',
                 'virtio-iommu-pci,id=i,addr=5,granule=4k,aw-bits=48,boot-bypass=off']
                if arm else ['-device', 'intel-iommu,id=i,intremap=on,eim=on,aw-bits=48,caching-mode=on,device-iotlb=off'])
    backend = f'blkdebug:{debug}:{image}' if debug is not None else str(image)
    return ['sh', str(ROOT/'scripts/qemu.sh'), *controls, *topology, '-drive',
            f'if=none,id=d,file={backend},format=raw,cache=writeback,werror=report,rerror=report,readonly={"on" if readonly else "off"}',
            '-device', f'virtio-blk-pci,id=b,addr=6,drive=d,disable-legacy=on,iommu_platform=on,ats=off,'
            f'num-queues=1,vectors={0 if arm else 2},logical_block_size={sector},physical_block_size={sector}']

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('image', type=Path)
    parser.add_argument('--sector', choices=(512,4096), type=int, default=512)
    args = parser.parse_args()
    arch = os.environ.get('NOVA_ARCH', 'x86_64')
    iso = ROOT/'build'/f'bloc-live-{arch}.iso'
    subprocess.run(['sh',ROOT/'scripts/iso.sh','storage-live',iso],check=True)
    argv=command(arch,args.sector,args.image,readonly=True,controls=['-serial','stdio'])
    os.execvpe(argv[0],argv,{**os.environ,'NOVA_ARCH':arch,'NOVA_ISO':str(iso)})
