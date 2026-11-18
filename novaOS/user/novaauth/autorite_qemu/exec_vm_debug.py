"""Contrôle QMP/GDB de laboratoire : arrêt CPU, aucune écriture dans la mémoire invitée.

Les points d'arrêt QEMU observent le chemin réel ; ils ne créent ni ticket ni droit.
"""
import json,os,select,socket,subprocess,time
from pathlib import Path
from reprise import Refus

def qmp(path,operation):
    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as s:
        s.settimeout(10);s.connect(str(path))
        with s.makefile('rwb',buffering=0) as stream:
            json.loads(stream.readline())
            for op in ('qmp_capabilities',operation):
                stream.write((json.dumps({'execute':op})+'\n').encode())
                while True:
                    row=json.loads(stream.readline())
                    if 'error' in row:raise Refus('QMP '+str(row['error']))
                    if 'return' in row:break
            return row['return']

def symbol(kernel,name):
    rows=subprocess.check_output(['/opt/homebrew/opt/llvm/bin/llvm-nm','--demangle',str(kernel)],text=True).splitlines()
    matches=[int(row.split()[0],16) for row in rows if row.endswith(' '+name)]
    if len(matches)!=1:raise Refus('symbole absent ou ambigu : '+name)
    return matches[0]

class Debug:
    def __init__(self,path,serial,arch):
        self.s=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM);self.s.settimeout(10)
        self.s.connect(str(path));self.serial=serial;self.arch=arch;self.buffer=b''
    def close(self):self.s.close()
    def send(self,data):
        raw=data.encode('ascii');self.s.sendall(b'$'+raw+b'#'+f'{sum(raw)%256:02x}'.encode())
    def packet(self,seconds=20):
        end=time.monotonic()+seconds
        while True:
            self.buffer=self.buffer.lstrip(b'+')
            if self.buffer.startswith(b'-'):raise Refus('GDB demande retransmission')
            if b'$' in self.buffer and b'#' in self.buffer:
                at=self.buffer.index(b'$');stop=self.buffer.index(b'#',at)
                if len(self.buffer)>=stop+3:
                    data=self.buffer[at+1:stop];checksum=self.buffer[stop+1:stop+3];self.buffer=self.buffer[stop+3:]
                    if int(checksum,16)!=sum(data)%256:raise Refus('GDB checksum invalide')
                    self.s.sendall(b'+');return data.decode('ascii')
            remaining=end-time.monotonic()
            if remaining<=0:raise TimeoutError('point GDB non atteint')
            ready,_,_=select.select([self.s,self.serial.vm.stdout],[],[],remaining)
            if not ready:raise TimeoutError('point GDB non atteint')
            if self.serial.vm.stdout in ready:
                data=os.read(self.serial.vm.stdout.fileno(),32768)
                if not data:raise EOFError('invité terminé pendant contrôle GDB')
                self.serial.data.extend(data);self.serial.trace.write(data);self.serial.trace.flush()
            if self.s in ready:
                data=self.s.recv(32768)
                if not data:raise EOFError('GDB fermé')
                self.buffer+=data
    def command(self,data):self.send(data);return self.packet()
    def breakpoint(self,address,add=True):
        value=self.command(f'{"Z" if add else "z"}0,{address:x},{4 if self.arch=="aarch64" else 1}')
        if value!='OK':raise Refus('point GDB refusé : '+value)
    def resume(self):self.send('c')
    def stopped(self):
        result=self.packet(30)
        if not result.startswith(('T05','S05')):raise Refus('arrêt GDB inattendu : '+result)
        return result
    def register(self,index):
        raw=self.command(f'p{index:x}')
        if not raw or raw.startswith('E'):raise Refus('registre GDB indisponible')
        return int.from_bytes(bytes.fromhex(raw),'little')
