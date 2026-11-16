"""Encode les octets déjà observés pour le banc SDK sans système de fichiers."""
import struct

class Vectors:
    def __init__(self):
        self.pool={}
        self.records=[]
        self.ids=[]
    def intern(self,data):
        if data not in self.pool:self.pool[data]=len(self.pool)
        return self.pool[data]
    def append(self,row,work,files):
        args=row["args"]
        # Les pannes de Darwin et ses offsets creux appartiennent à son adaptateur.
        if row["stdin_regulier"] or any(arg in (".","interdit","sparse1","sparse2","fichier avec espace") for arg in args):return
        data=[self.intern(arg.encode()) for arg in args]
        data=[len(data),*data,self.intern(bytes.fromhex(row["stdin"]))]
        names=[]
        for arg in sorted(set(args)):
            path=work/arg
            if arg and arg!="-" and path.is_file():
                names.extend([self.intern(arg.encode()),self.intern(path.read_bytes())])
        oracle=row["mesures"]["Nova" if row["document"] else "GNU"]
        expected=row["attendu"] if row["attendu"] is not None else oracle[0]
        data.extend([len(names)//2,*names,self.intern(row["lang"].encode()),expected,
                     self.intern(bytes.fromhex(oracle[1])),self.intern(bytes.fromhex(oracle[2]))])
        self.records.append(data)
        self.ids.append(row["id"])
    def save(self,path):
        with path.open("wb") as f:
            def integer(v):f.write(struct.pack("<I",v))
            integer(len(self.pool))
            for data in self.pool:integer(len(data));f.write(data)
            integer(len(self.records))
            for row in self.records:
                for val in row:integer(val)
