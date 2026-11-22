"""Réception HPKE 2-sur-2 réelle dans R, sans activation P9 : étape cryptographique.

Les témoins chiffrés synthétiques ne qualifient pas la recette VFS anti-rollback.
Aucune part privée ne passe dans le processus TLS ou dans les preuves.
"""
import argparse
from contextlib import ExitStack
import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import socket
import ssl
import struct
import subprocess
import sys
import tempfile
import threading
import time

ROOT=Path(__file__).resolve().parents[4]
sys.path[:0]=[str(ROOT/"novaOS/user/novaauth/service"),
             str(ROOT/"novaOS/user/novaauth/transport_qemu")]
from adaptateur import servir,lire
from paquet_qemu import Serial
from shell_qemu import command
from acteurs_recuperation import Actor,holder_worker,administrator_worker,public
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from reprise import initialize as init_authority,Recovery,Refus
from lecture_tete import respond
from recuperation import RecoveryAuthority,initialize,REGISTER,digest
from cles_recuperation import Enrollment
from rpc_recuperation import Dispatcher,decode_header,header
from mutation_rpc import request as mutation_request
from capture_rpc import CaptureStore

SERVER=b"NovaOS-QEMU-Srv1"
DOMAIN=bytes([7])*16

def sha(path):
    with path.open("rb") as f:
        return hashlib.file_digest(f,"sha256").hexdigest()

def fixture(args,stack):
    # Ces clés de test ne sont pas des secrets de production. Les détenteurs
    # créent néanmoins leurs secrets dans leurs seuls processus.
    holders=[Actor(holder_worker,i,args.secrets/str(i),args.detenteur) for i in (1,2)]
    for h in holders:stack.callback(h.close)
    administrator=Actor(administrator_worker,args.secrets/"administrateur")
    stack.callback(administrator.close)
    backup=[h.keys[0] for h in holders]
    request=[h.keys[1] for h in holders]
    signing=[h.keys[2] for h in holders]
    decision=Ed25519PrivateKey.from_private_bytes(args.decision.read_bytes())
    # Provisionnement synthétique antérieur au démarrage ; ce n'est pas une
    # signature du registre invité ni une preuve de capture du vrai triplet.
    server=Ed25519PrivateKey.from_private_bytes((args.cles/"serveur").read_bytes())
    objects=[b"identite",b"groupes!",b"facteur!"]
    volume=[b"v"*16,b"l"*16];object_ids=[10,11,12]
    inner=hashlib.sha256(b"NovaAuth/head/1"+DOMAIN+b"".join(volume)+b"".join(
        struct.pack("<QQ",oid,len(value))+value for oid,value in zip(object_ids,objects))).digest()
    native_head=hashlib.sha256(b"NVAUTH01"+bytes([3])+DOMAIN+struct.pack("<Q",13)+b"q"*16+inner).digest()
    head=DOMAIN+b"".join(volume)+b"r"*16+struct.pack(">QQQQ",*object_ids,13)+b"q"*16+native_head
    if args.triplet is not None:
        raw=args.triplet.read_bytes()
        if raw[:8]!=b"NATRIP01" or len(raw)<164:raise Refus("triplet natif incomplet")
        head=raw[8:152];at=152;objects=[]
        for _ in range(3):
            if at+4>len(raw):raise Refus("longueur native absente")
            n=int.from_bytes(raw[at:at+4],"big");at+=4
            if not 0<n<=16331 or at+n>len(raw):raise Refus("objet natif hors borne")
            objects.append(raw[at:at+n]);at+=n
        if at!=len(raw):raise Refus("suffixe natif invalide")
        native_head=head[112:144]
    generation=int.from_bytes(head[88:96],"big")
    source=SERVER+DOMAIN+head+struct.pack(">Q",2)+b"".join(backup)+struct.pack(">III",*[len(x) for x in objects])+b"".join(objects)
    result=subprocess.check_output([str(args.capture)],input=source,timeout=10)
    manifest=result[:418];p=418;chunks=[]
    while p<len(result):
        n=int.from_bytes(result[p:p+4],"big");p+=4
        if not 17<=n<=1040 or len(result[p:p+n])!=n:raise Refus("capture tronquée")
        chunks.append(result[p:p+n]);p+=n
    database=args.sortie/"autorite-independante.sqlite"
    init_authority(database,public(decision))
    old=Recovery(database,decision)
    old.provision(SERVER,DOMAIN,b"i"*32,generation,native_head,public(server),administrator.keys[0])
    initialize(database)
    measurement=bytes.fromhex(sha(args.registre))
    authority=RecoveryAuthority(old,[measurement])
    authority.provision_keys(SERVER,DOMAIN,2,backup+signing)
    authority.register_backup(manifest,b"i"*32,server.sign(REGISTER+b"i"*32+manifest))
    keys=[backup[0],backup[1],request[0],request[1],signing[0],signing[1]]
    administrator.call("configure",SERVER,DOMAIN,public(server),measurement,digest(manifest),generation,native_head,keys)
    for holder in holders:
        holder.call("configure",public(decision),public(server),administrator.keys[0],[measurement])
    enrollment=Enrollment(authority,args.detenteur)
    packet,envelopes=enrollment.challenge(SERVER,DOMAIN,2,request)
    proofs=[holders[i].call("enroll",packet,envelopes[i]) for i in range(2)]
    enrollment.register(packet,proofs[0][0],proofs[1][0],
                        administrator.call("enroll",packet),[x[1] for x in proofs])
    def approve(c,server_signature,pop):
        if args.mode=="non-approuvee":raise Refus("incarnation non approuvée par administrateur")
        return administrator.call("approve",c,server_signature,pop)
    previous=[]
    def deliver(role,*fields):
        if args.mode=="part-expiree" and role==2:
            authority.clock=lambda:time.monotonic_ns()+301_000_000_000
        if args.mode=="sauvegarde-differente" and role==2:
            fields=list(fields);m=bytearray(fields[1]);m[46]^=1;fields[1]=bytes(m)
        if args.mode=="part-rejouee" and role==2:
            return previous[0]
        if args.mode=="part-absente" and role==2:
            raise Refus("détenteur A indisponible")
        envelope,_=holders[role-1].call("deliver",*fields,authority=authority,
            lose_record_reply=(args.mode==f"recu-perdu-{role}"))
        if args.mode=="part-alteree" and role==2:
            envelope=envelope[:-1]+bytes([envelope[-1]^1])
        previous.append(envelope)
        return envelope
    def retrieve(role,*fields):
        envelope,_=holders[role-1].call("outcome",*fields,authority=authority)
        return envelope
    (args.sortie/"manifeste.bin").write_bytes(manifest)
    for i,chunk in enumerate(chunks):(args.sortie/f"fragment-{i}.chiffre").write_bytes(chunk)
    dispatcher=Dispatcher(authority,enrollment,SERVER,DOMAIN,public(server),measurement,
                          {digest(manifest):manifest},{digest(manifest):chunks},approve,deliver,retrieve)
    return dispatcher,holders,administrator

def run(args,environment=None,before_mutation=None,scenario=None,rpc_hook=None):
    out=args.sortie
    out.mkdir(parents=True,exist_ok=False)
    # Une coupure peut précéder la première requête applicative : zéro ligne
    # constitue une preuve, pas un journal manquant.
    (out/"rpc-public.log").touch(exist_ok=False)
    if environment is None:args.secrets.mkdir(mode=0o700,parents=True,exist_ok=False)
    disk=out/"identites.p9"
    shutil.copyfile(args.genese,disk)
    legacy=None
    if getattr(args,"legacy_disk",None) is not None:
        legacy=out/"ancienne-autorite-locale.raw"
        shutil.copyfile(args.legacy_disk,legacy)
    with tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]) as check:
        extracted=Path(check)/"noyau.elf"
        subprocess.run(["xorriso","-osirrox","on","-indev",str(args.iso),"-extract","/boot/novaos.elf",str(extracted)],
            stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
        if args.registre.read_bytes() not in extracted.read_bytes():raise Refus("empreinte registre étrangère à l'ISO")
    observed={"banc_sha256":sha(Path(__file__)),"registre_inclus_dans_iso":True,"architecture":args.arch,"secteur":args.sector,"mode":args.mode,
              "portee":"réception cryptographique invitée ; aucune activation P9 ou qualification VFS",
              "triplet_prepare":args.triplet is not None,
              "iso_sha256":sha(args.iso),"registre_sha256":sha(args.registre),
              "p9_initial_sha256":sha(disk),"statut":"échec",
              "version_qemu":subprocess.check_output(["qemu-system-"+args.arch,"--version"],text=True).splitlines()[0],
              "vcpu":2,"ram_mio":256,"acceleration":"tcg","reseau":"none",
              "qemu_script_sha256":sha(ROOT/"novaOS/scripts/qemu.sh")}
    with ExitStack() as stack:
        dispatcher,holders,administrator=fixture(args,stack) if environment is None else environment
        capture_store=CaptureStore(out/"captures-invite",dispatcher.authority,dispatcher.peer_key,(SERVER,DOMAIN))
        observed["pid_detenteurs"]=[h.process.pid for h in holders]
        observed["pid_administrateur"]=administrator.process.pid
        ctx=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.minimum_version=ctx.maximum_version=ssl.TLSVersion.TLSv1_3
        ctx.set_groups("P-256");ctx.set_client_sigalgs("ed25519");ctx.set_server_sigalgs("ed25519")
        ctx.set_ciphersuites("TLS_AES_128_GCM_SHA256")
        ctx.verify_mode=ssl.CERT_REQUIRED;ctx.num_tickets=0
        ctx.load_cert_chain(args.cles/"autorite.pem",args.cles/"autorite-cle.pem")
        ctx.load_verify_locations(args.cles/"ca.pem")
        def audit(conn,direction,version,kind,code,data):
            if int(code)==8 and int(kind)==22 and direction=="write":
                raw=memoryview(data)[6:]
                while len(raw)>=4:
                    ext=int.from_bytes(raw[:2],"big");n=int.from_bytes(raw[2:4],"big")
                    if n>len(raw)-4:raise ValueError("extension TLS tronquée")
                    if ext==1:observed["fragment_2048"]=bytes(raw[4:4+n])==b"\x03"
                    raw=raw[4+n:]
            with (out/"tls-public.log").open("a") as f:
                print(direction,int(version),int(kind),int(code),len(data),file=f)
        ctx._msg_callback=audit
        listener=stack.enter_context(socket.socket(socket.AF_UNIX,socket.SOCK_STREAM))
        tmp=stack.enter_context(tempfile.TemporaryDirectory(dir=os.environ["NOVAOS_TMPDIR"]))
        # Chemin court requis par sockaddr_un.
        os.chdir(tmp);listener.bind("port.sock");os.chdir(ROOT)
        listener.listen(1);listener.settimeout(70)
        left,right=socket.socketpair()
        reopen=None
        if args.vue:
            reopen=stack.enter_context(socket.socket(socket.AF_UNIX,socket.SOCK_STREAM))
            reopen.bind(str(Path(tmp)/"relai.sock"));reopen.listen(1)
        stop_host=threading.Event()
        active_connections=[]
        def host():
            try:
                stream=right
                historical_limit=128 if getattr(args,"exec_admission",False) else (64 if args.mutation else (32 if args.vfs else (16 if args.vue else 1)))
                connection_limit=getattr(args,"transport_connection_limit",historical_limit)
                if type(connection_limit) is not int or not 1<=connection_limit<=320:
                    raise ValueError("budget de connexions du banc hors borne 1..320")
                observed["limite_connexions_banc"]=connection_limit
                observed["connexions_banc"]=0
                for attempt in range(connection_limit):
                    if stop_host.is_set():break
                    if attempt:
                        stream=socket.socket(socket.AF_UNIX,socket.SOCK_STREAM)
                        stream.settimeout(10);stream.connect(str(Path(tmp)/"relai.sock"))
                    with stream,ctx.wrap_socket(stream,server_side=True,do_handshake_on_connect=False) as conn:
                        process_connection(conn)
                        observed["connexions_banc"]+=1
                else:
                    observed["limite_connexions_banc_atteinte"]=True
            except Exception as error:
                if not stop_host.is_set():observed["erreur_tls"]=type(error).__name__+": "+str(error)
        def process_connection(conn):
                    active_connections.append(conn)
                    # Le relais préconnecte le prochain socket. L'attente de
                    # ClientHello n'est pas une négociation TLS commencée : le
                    # témoin peut attendre l'expiration d'un bail de 60 secondes.
                    # Le banc global reste borné ; l'arrêt interrompt cette attente.
                    waiting=time.monotonic_ns()
                    while not select.select([conn],[],[],0.1)[0]:
                        if stop_host.is_set():return
                    ready=time.monotonic_ns()
                    with (out/"tls-attentes.log").open("a") as f:
                        print(waiting,ready,ready-waiting,file=f)
                    clock=getattr(args,"pause_transport",None)
                    if clock is not None:
                        from horloge_pause_qemu import Stream
                        conn=Stream(conn,clock)
                    else:conn.settimeout(45)
                    conn.do_handshake()
                    if conn.getpeercert(True)!=(args.cles/"serveur.der").read_bytes() or conn.session_reused:
                        raise Refus("pair TLS non épinglé")
                    observed.update(tls=conn.version(),certificat_epingle=True)
                    if getattr(args,"autorite_hors_ligne",False):
                        observed["autorite_hors_ligne"]=True
                        conn.shutdown(socket.SHUT_RDWR)
                        return
                    while True:
                        try:h=bytes(lire(conn,16))
                        except EOFError:break
                        if args.vue and h[:8]==b"NACURR01":
                            request=h+bytes(lire(conn,104-len(h)))
                            decision=Ed25519PrivateKey.from_private_bytes(args.decision.read_bytes())
                            if before_mutation is not None:
                                before_mutation(disk,out,Path(tmp)/"qmp.sock",request,"consultation")
                            response=respond(conn,request,dispatcher.authority.path,decision)
                            if not (out/"tete-historique.bin").exists():
                                (out/"tete-historique.bin").write_bytes(response)
                            conn.sendall(response)
                            observed["consultations_tete_fraiche"]=observed.get("consultations_tete_fraiche",0)+1
                            break
                        sequence,operation,n=decode_header(h)
                        body=bytes(lire(conn,n))
                        start=time.monotonic_ns()
                        try:
                            if rpc_hook is not None:
                                rpc_hook("before",operation,body,None,Path(tmp)/"qmp.sock",disk)
                            if getattr(args,"exec_admission",False) and operation in (40,41,42):
                                from admission_rpc import request as exec_request
                                result=exec_request(dispatcher.authority.path,
                                    Ed25519PrivateKey.from_private_bytes(args.decision.read_bytes()),
                                    (args.cles/"serveur.pub").read_bytes(),(SERVER,DOMAIN),operation,body,
                                    administration=getattr(args,"administration_r2",False))
                            elif getattr(args,"administration_r2",False) and operation in (50,51,52,53):
                                from administration_mutation_rpc import request as admin_request
                                result=admin_request(dispatcher.authority.path,
                                    Ed25519PrivateKey.from_private_bytes(args.decision.read_bytes()),
                                    (args.cles/"serveur.pub").read_bytes(),(SERVER,DOMAIN),operation,body,
                                    administration=True)
                            elif args.vfs and operation in (32,33,34):
                                result=capture_store.request(operation,body)
                            elif (args.vfs and operation in (30,31)) or (getattr(args,"administration_r2",False) and operation==31):
                                if operation==30 and before_mutation is not None:
                                    before_mutation(disk,out,Path(tmp)/"qmp.sock",body,"publication")
                                result=mutation_request(dispatcher.authority.path,
                                    Ed25519PrivateKey.from_private_bytes(args.decision.read_bytes()),
                                    (args.cles/"serveur.pub").read_bytes(),(SERVER,DOMAIN),operation,body)
                            else:result=dispatcher.request(sequence,operation,body)
                            if rpc_hook is not None:
                                rpc_hook("after",operation,body,result,Path(tmp)/"qmp.sock",disk)
                            if operation==10 and getattr(args,"tete_rejouee",None) is not None:
                                result=args.tete_rejouee.read_bytes()
                                observed["tete_historique_rejouee"]=sha(args.tete_rejouee)
                            # Attaque du relais : altérer sans la signature correspondante.
                            if args.mode=="cle-substituee" and operation==11:
                                b=bytearray(result);b[112]^=1;result=bytes(b)
                            if args.mode=="generation-differente" and operation==13:
                                b=bytearray(result);b[135]^=1;result=bytes(b)
                            status=22 if ((args.mode=="activation-perdue" and operation==18) or
                                (args.mode in ("publication-perdue","publication-capture-perdues") and operation==30) or
                                (args.mode=="publication-capture-perdues" and operation==32)) else 0
                            if status==22:result=b""
                        except Refus as error:
                            status=22 if "UNCERTAIN" in str(error) else 5;result=b""
                            with (out/"refus.log").open("a") as f:print(sequence,operation,str(error),file=f)
                        with (out/"rpc-public.log").open("a") as f:
                            print(sequence,operation,n,len(result),status,start,time.monotonic_ns(),file=f)
                        payload=status.to_bytes(8,"little")+result
                        conn.sendall(header(sequence,operation|0x8000,len(payload))+payload)
        def adapter():
            try:
                with disk.open("r+b",buffering=0) as f,(out/"nbd.log").open("x") as log:
                    servir(f.fileno(),listener,left,args.sector,lambda *a:print(*a,file=log,flush=True),reopen=reopen)
            except Exception as error:
                observed["erreur_nbd"]=type(error).__name__+": "+str(error)
        workers=[threading.Thread(target=host),threading.Thread(target=adapter)]
        for worker in workers:worker.start()
        vm_args=command(args.arch,args.sector,disk,controls=["-display","none","-monitor","none","-serial","stdio","-qmp","unix:qmp.sock,server=on,wait=off"])
        vm_args[vm_args.index("-drive")+1]="if=none,id=d,format=raw,file.driver=nbd,file.server.type=unix,file.server.path=port.sock,cache=none,werror=report,rerror=report"
        if getattr(args,"exec_admission",False):
            vm_args += ["-gdb","unix:gdb.sock,server=on,wait=off"]
            if getattr(args,"collecteur_r2",False):vm_args += ["-S"]
        if legacy is not None:
            # Ancien support local restaurable ; jamais utilisé comme décideur.
            vm_args += ["-drive",f"if=none,id=legacy,format=raw,file={legacy},cache=none",
                "-device",f"virtio-blk-pci,id=legacy-device,addr=7,drive=legacy,disable-legacy=on,iommu_platform=on,ats=off,num-queues=1,vectors=0,logical_block_size={args.sector},physical_block_size={args.sector}"]
            observed["ancienne_autorite_initiale_sha256"]=sha(legacy)
        env=dict(os.environ,NOVA_ISO=str(args.iso),NOVA_ARCH=args.arch,NOVA_ARM_CPU="max",NOVA_VCPUS="2")
        observed["commande_qemu"]=vm_args
        start=time.monotonic_ns()
        with (out/"invite.serial").open("xb") as raw,subprocess.Popen(vm_args,cwd=tmp,env=env,
                stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,bufsize=0) as vm:
            try:
                t=Serial(vm,raw)
                if scenario is not None:
                    observed["scenario_admissions"]=scenario(t,Path(tmp),disk,out)
                    observed["statut"]="diagnostic terminé ; consulter succes et scenario_admissions"
                elif getattr(args,"quarantaine",False):
                    t.expect(rb"NARV quarantaine terminee",100)
                    observed["statut"]="quarantaine"
                elif args.mode not in ("nominal","recu-perdu-1","recu-perdu-2","activation-perdue","publication-perdue","publication-capture-perdues"):
                    match=t.expect(rb"NAR2 registre termine statut=([0-9a-f]{16})",65)
                    observed["statut_natif"]=int(match[1],16)
                    observed["statut"]="refus invité"
                    # Le signataire ferme sa route et le transport quand R quitte.
                    closed = rb"NAUTH mesure NAUTH TLS apres roots=\d+ handles=\d+ waits=\d+ pages=\d+ pile=\d+ cadres_ipc_globaux=\d+\r?\n"
                    if not re.search(closed,t.data):
                        t.expect(closed,15)
                else:
                    if args.vfs:
                        t.expect(rb"NARV parcours termine",100)
                    elif args.vue:
                        t.expect(rb"NAR2 vue registre isole integre aucune decision locale NovaFS",65)
                        t.submit("echo NAR_VUE_AUTHENTIFIE; echo NAR_VUE_RC_$?")
                        t.expect(rb"NAR_VUE_AUTHENTIFIE\r?\n",30)
                        match=t.expect(rb"NAR_VUE_RC_([0-9]+)\r?\n",30)
                        observed["statut_natif_authentification"]=int(match[1])
                        t.expect(rb"nsh> ",10)
                    else:
                        t.expect(rb"NAR2 triplet dechiffre dans registre aucune activation P9",65)
                        t.expect(rb"NAR2 dialogue ferme delai global inchange",10)
                    observed["statut"]="réception complète"
                if getattr(args,"mesures_finales",False):
                    # Le terminal n'est pas authentifié dans cette recette.
                    # Aucun contournement de sa garde : arrêt contrôlé QEMU
                    # après réception de toutes les issues et FLUSH P9.
                    with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as monitor:
                        monitor.settimeout(10);monitor.connect(str(Path(tmp)/"qmp.sock"))
                        with monitor.makefile("rwb",buffering=0) as channel:
                            json.loads(channel.readline())
                            def qmp(op):
                                channel.write((json.dumps({"execute":op})+"\n").encode())
                                while True:
                                    row=json.loads(channel.readline())
                                    if "error" in row:raise Refus("QMP: "+str(row["error"]))
                                    if "return" in row:return row["return"]
                            qmp("qmp_capabilities");qmp("stop")
                            if qmp("query-status")["status"]!="paused" or qmp("query-block-jobs"):
                                raise Refus("invité ou opérations bloc non arrêtés")
                            with disk.open("rb") as final_disk:os.fsync(final_disk.fileno())
                            qmp("quit")
                    observed["arret_qemu_apres_acquittements"]=vm.wait(10)==0
                    observed["arret_systeme_par_nsh"]=False
            except Exception as error:
                observed["erreur_invite"]=str(error)
            finally:
                stop_host.set()
                if vm.poll() is None:vm.terminate()
                vm.wait(5);raw.write(vm.stdout.read())
        observed["duree_hote_ns"]=time.monotonic_ns()-start
        if getattr(args,"pause_transport",None) is not None:
            observed["suspension_qmp_hote_exclue_s"]=args.pause_transport.excluded()
            observed["delai_tls_hote_actif_s"]=45

        # Réveiller les accept/read du relais après arrêt de l'invité. Aucun
        # worker hôte ni socket de test ne doit rester en arrière-plan.
        for channel in [*active_connections,left,right,listener,reopen]:
            if channel is not None:
                try:channel.shutdown(socket.SHUT_RDWR)
                except OSError:pass
        for worker in workers:worker.join(10)
        observed["relais_draine"]=not any(worker.is_alive() for worker in workers)
    raw=(out/"invite.serial").read_bytes()
    if legacy is not None:
        observed["ancienne_autorite_finale_sha256"]=sha(legacy)
        observed["ancienne_autorite_inchangee"]=observed["ancienne_autorite_initiale_sha256"]==sha(legacy)
    observed["p9_final_sha256"]=sha(disk)
    observed["p9_inchange"]=observed["p9_initial_sha256"]==observed["p9_final_sha256"]
    snapshots=re.findall(rb"NAUTH mesure NAUTH TLS (avant|actif|apres) roots=(\d+) handles=(\d+) waits=(\d+) pages=(\d+) pile=(\d+) cadres_ipc_globaux=(\d+)",raw)
    observed["ressources"]={row[0].decode():list(map(int,row[1:])) for row in snapshots}
    res=observed["ressources"]
    observed["drainage_signataire"]=set(res)=={"avant","actif","apres"} and res["avant"][:5]==res["apres"][:5] and res["apres"][2]==0
    rows=re.findall(rb"NAR2 mesure NAR2 registre (avant|apres) roots=(\d+) handles=(\d+) waits=(\d+) pages=(\d+) pile=(\d+) cadres_ipc_globaux=(\d+)",raw)
    observed["ressources_registre"]={row[0].decode():list(map(int,row[1:])) for row in rows}
    res_r=observed["ressources_registre"]
    observed["drainage_registre"]=(set(res_r)=={"avant","apres"} and
        res_r["avant"][:5]==res_r["apres"][:5] and res_r["apres"][2]==0)
    if getattr(args,"autorite_hors_ligne",False):
        # EOF TLS entraîne la sortie du signataire : le noyau retire alors
        # précisément sa feuille du registre (7 -> 6), sans nouvelle racine.
        # Ce témoin est distinct de l'égalité exigée quand le pair survit.
        signer=re.search(rb"DEBUG pid=(\d+) NAUTH signataire NOW autorise",raw)
        retired=bool(signer and re.search(rb"PROC pid="+signer[1]+rb" .* sortie=12\r?\n",raw))
        wanted=list(res_r.get("avant",[])[:5])
        if len(wanted)==5:wanted[1]-=1
        observed["feuille_signataire_retiree"]=(retired and wanted==res_r.get("apres",[])[:5]
            and res_r.get("apres",[1,1,1])[2]==0)
        observed["drainage_registre"]=observed["feuille_signataire_retiree"]
    roots=re.findall(rb"NAUTH install pid=(\d+) roots=(\d+)",raw)
    peaks={}
    for pid,count in roots:
        name=pid.decode();peaks[name]=max(peaks.get(name,0),int(count))
    observed["pics_racines_par_pid"]=peaks
    observed["plafond_respecte"]=bool(peaks) and max(peaks.values())<=4
    observed["cinquieme_racine_refusee"]=len(re.findall(rb"NAUTH saturation pid=.*statut=Err\(6\)",raw))==2
    times=re.search(rb"NAUTH TLS debut=([0-9a-f]+) echeance=([0-9a-f]+) fin=([0-9a-f]+)",raw)
    timely=False
    if times:
        start,limit,end=(int(v,16) for v in times.groups())
        observed.update(debut_ms=start,echeance_ms=limit,fin_ms=end)
        timely=limit-start==10000 and start<=end<limit
    negative=args.mode not in ("nominal","recu-perdu-1","recu-perdu-2","activation-perdue","publication-perdue","publication-capture-perdues")
    rows=[list(map(int,line.split())) for line in (out/"rpc-public.log").read_text().splitlines()]
    observed["mutations_remise"]=sum(r[1]==16 for r in rows)
    observed["consultations_issue"]=sum(r[1]==22 for r in rows)
    observed["reconciliation_sans_repetition"]=(
        observed["mutations_remise"]==2 and observed["consultations_issue"]==1)
    expected=(observed["statut"]=="réception complète" and b"NAR2 empreinte native triplet conforme tete durable" in raw if not negative else
        observed["statut"]=="refus invité" and observed.get("statut_natif",0)!=0
        and b"NAR2 triplet dechiffre" not in raw)
    if args.triplet is not None:
        observed["portee"]="préparation/relecture privée du triplet dans P9 ; aucune activation ni qualification VFS"
        expected=expected and b"NAR2 preparation P9 relue empreinte conforme aucune activation" in raw
    if args.activation:
        observed["portee"]="triplet P9 relu, reçu d’activation et tête fraîche vérifiés ; VFS non raccordé"
        observed["activation_verifiee"]=b"NAR2 activation distante recue et tete fraiche validees VFS non raccorde" in raw
        observed["mutations_activation"]=sum(r[1]==18 for r in rows)
        observed["consultations_activation"]=sum(r[1]==20 for r in rows)
        expected=expected and observed["activation_verifiee"] and observed["mutations_activation"]==1
        if args.mode=="activation-perdue":
            expected=expected and observed["consultations_activation"]==1 and b"NAR2 activation UNCERTAIN issue retrouvee sans repetition" in raw
    if args.vue and not args.vfs:
        observed["portee"]="vue isolée relue et authentification réelle de nsh ; recette VFS non raccordée"
        expected=expected and observed.get("statut_natif_authentification")==0 and observed.get("consultations_tete_fraiche",0)>=4
    if args.vfs:
        complete={name.decode():bytes.fromhex(value.decode()) for name,value in
            re.findall(rb"NARV ((?:Alice|Bob) [a-zA-Z ]+) ([0-9a-f]{48,128})",raw)}
        records={name:value[:24] for name,value in complete.items()}
        sessions={name:dict(statut=int.from_bytes(v[:8],"little"),identite=v[8:24].hex(),session=v[24:56].hex(),
            ancienne_session_statut=int.from_bytes(v[56:64],"little")) for name,v in complete.items() if "authentification" in name and len(v)==64}
        observed["sessions"]=sessions
        observed["sessions_distinctes"]=bool(sessions) and len({sessions[n]["session"] for n in
            ("Alice authentification","Alice nouvelle authentification") if n in sessions})==2
        observed["ancienne_session_refusee"]=("Alice nouvelle authentification" in sessions) and all(sessions[n]["ancienne_session_statut"]==5 for n in
            ("Alice nouvelle authentification","Alice authentification apres retrait") if n in sessions)
        observed["lectures_vfs"]={name:{"statut":int.from_bytes(value[:8],"little"),
            "octets":int.from_bytes(value[8:16],"little"),"contenu_hex":value[16:24].hex()} for name,value in records.items()}
        observed["lectures_vfs"]={k:v for k,v in observed["lectures_vfs"].items() if "authentification" not in k}
        expected=expected and all(records.get(n,b"x")[:8]==bytes(8) for n in (
            "Alice authentification","Bob authentification","Alice nouvelle authentification","Alice fermeture"))
        for who,expected_bytes,success in [("Alice P",b"ALICE-P\n",args.lecture_privee=="autorisee"),
            ("Alice nouvelle lecture P",b"ALICE-P\n",args.lecture_privee=="autorisee"),
            ("Alice L",b"ALICE-L\n",True),("Alice nouvelle lecture L",b"ALICE-L\n",True),
            ("Alice nouvelle lecture L avant P",b"ALICE-L\n",True),
            ("Bob P",b"",False)]:
            wanted=(bytes(8)+(8).to_bytes(8,"little")+expected_bytes) if success else (5).to_bytes(8,"little")+bytes(16)
            expected=expected and records.get(who)==wanted
        expected=expected and b"NARV VFS lecture rc=0 octets=8" in raw and b"NARV VFS refus octets=0" in raw
        sequence=[b"NARV Alice nouvelle authentification ",b"NARV Alice nouvelle lecture L avant P ",
            b"NARV Alice nouvelle lecture P ",b"NARV Alice nouvelle lecture L "]
        offsets=[m.start() if (m:=re.search(re.escape(marker)+rb'[0-9a-f]{48}',raw)) else -1 for marker in sequence]
        observed["sequence_meme_session_L_P_L"]=all(i>=0 for i in offsets) and offsets==sorted(set(offsets))
        expected=expected and observed["sequence_meme_session_L_P_L"]
        if args.mutation:
            for name,wanted in [("Alice authentification apres retrait",None),
                ("Alice P apres retrait",(5).to_bytes(8,"little")+bytes(16)),
                ("Alice L apres retrait",bytes(8)+(8).to_bytes(8,"little")+b"ALICE-L\n")]:
                expected=expected and (records.get(name)==wanted if wanted is not None else records.get(name,b"x")[:8]==bytes(8))
            operations=[r for r in rows if r[1]==30];queries=[r for r in rows if r[1]==31]
            observed["mutations_politique"]=len(operations);observed["consultations_mutation"]=len(queries)
            expected=expected and len(operations)==1
            if args.mode in ("publication-perdue","publication-capture-perdues"):
                expected=expected and len(queries)==1 and records.get("Alice retrait")== (22).to_bytes(8,"little")+bytes(16) and records.get("Alice reconciliation")==bytes(24)
            else:expected=expected and records.get("Alice retrait")==bytes(24)
            if args.mode=="publication-capture-perdues":
                operations32=[r for r in rows if r[1]==32];queries34=[r for r in rows if r[1]==34]
                observed["capture_mutations"]=len(operations32);observed["capture_reconciliations"]=len(queries34)
                expected=expected and len(operations32)==1 and len(queries34)==1
            if b"NARV Alice sauvegarde " in raw:
                expected=expected and records.get("Alice sauvegarde")==bytes(24)
                observed["capture_invitee"]=records.get("Alice sauvegarde")==bytes(24)
        observed["portee"]="récupération puis authentification et lectures/refus par VFS invité ; publications initiales hôtes, parcours complet de restauration non encore qualifié"
    observed["echanges_tls_horodates"]=[dict(debut_ms=int(a,16),echeance_ms=int(b,16),fin_ms=int(c,16))
        for a,b,c in re.findall(rb"NAUTH TLS debut=([0-9a-f]+) echeance=([0-9a-f]+) fin=([0-9a-f]+)",raw)]
    observed["limite_mesures_tls"]="Le quota DEBUG peut supprimer des lignes ; seuls les échanges horodatés sont mesurés, le délai absolu reste imposé dans le signataire."
    if getattr(args,"quarantaine",False):
        records={name.decode():bytes.fromhex(value.decode()) for name,value in
            re.findall(rb"NARV ((?:Alice|Bob) [a-zA-Z ]+) ([0-9a-f]{48})",raw)}
        expected=(observed["statut"]=="quarantaine" and
            records.get("Alice authentification")== (12).to_bytes(8,"little")+bytes(16) and
            records.get("Alice P quarantaine")== (5).to_bytes(8,"little")+bytes(16) and
            b"NARV VFS refus octets=0" in raw and b"NARV VFS lecture" not in raw and
            b"NAR2 vue registre isole" not in raw)
        observed["portee"]="VFS invité : aucune activation sans approbation, refus et zéro octet"
    captures={}
    for role,prefix in (("registre",b"NAR2"),("signataire",b"NAUTH")):
        rows=re.findall(prefix+rb" mesure "+prefix+rb" capture (avant|apres) roots=(\d+) handles=(\d+) waits=(\d+) pages=(\d+) pile=(\d+) cadres_ipc_globaux=(\d+)",raw)
        captures[role]={r[0].decode():list(map(int,r[1:])) for r in rows}
    observed["ressources_capture"]=captures
    if getattr(args,"mesures_finales",False) and not getattr(args,"quarantaine",False):
        expected=expected and observed.get("arret_qemu_apres_acquittements") and observed.get("sessions_distinctes") and observed.get("ancienne_session_refusee")
        if args.mutation:
            expected=expected and all(set(v)=={"avant","apres"} and v["avant"][:5]==v["apres"][:5] for v in captures.values())
    observed["succes"]=bool(expected and observed.get("ancienne_autorite_inchangee",True) and observed.get("relais_draine") and observed.get("tls")=="TLSv1.3"
        and observed.get("fragment_2048") and observed["drainage_signataire"]
        and observed["drainage_registre"] and observed["plafond_respecte"] and observed["cinquieme_racine_refusee"]
        and timely and b"NOW autorise gestion et arguments refuses 8/8" in raw
        and (not args.mode.startswith("recu-perdu") or observed["reconciliation_sans_repetition"]))
    if scenario is not None:
        # Recette distincte : ne pas attribuer les contrôles du scénario VFS
        # historique à un invité exécutant le témoin de tickets dans nsh.
        observed["portee"]="tickets EXEC_TICKET invités, scénario explicite ; recettes VFS distinctes"
        observed["succes"]=bool(observed.get("scenario_admissions",{}).get("succes") and
            observed.get("relais_draine") and observed.get("ancienne_autorite_inchangee",True) and
            observed.get("tls")=="TLSv1.3" and observed["plafond_respecte"])
    (out/"resultat.json").write_text(json.dumps(observed,indent=2,ensure_ascii=False)+"\n")
    print(json.dumps(observed,ensure_ascii=False))
    return 0 if observed["succes"] else 1

def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument("arch",choices=["aarch64","x86_64"])
    p.add_argument("sector",type=int,choices=[512,4096])
    p.add_argument("mode",choices=["nominal","part-absente","part-alteree","recu-perdu-1","recu-perdu-2","cle-substituee","non-approuvee","sauvegarde-differente","generation-differente","part-expiree","part-rejouee","activation-perdue","publication-perdue","publication-capture-perdues"])
    for name in ("iso","registre","genese","cles","decision","secrets","sortie","detenteur","capture"):
        p.add_argument("--"+name,type=lambda x:Path(x).resolve(),required=True)
    p.add_argument("--triplet",type=lambda x:Path(x).resolve(),help="triplet natif synthétique de l’effecteur, pas une recette VFS")
    p.add_argument("--mutation",action="store_true",help="retrait réel par le registre invité et reçu indépendant")
    p.add_argument("--vfs",action="store_true",help="lectures binaires par Alice et refus de Bob")
    p.add_argument("--lecture-privee",choices=["autorisee","refusee"],default="refusee")
    p.add_argument("--vue",action="store_true",help="consultation du registre isolé avec nouvelle liaison TLS")
    p.add_argument("--activation",action="store_true",help="exiger reçu durable, tête fraîche et absence de répétition")
    args=p.parse_args()
    if args.mutation:args.vfs=True
    if args.vfs:args.vue=True
    if args.vue:args.activation=True
    if args.activation and args.triplet is None:p.error("activation exige triplet natif")
    return run(args)

if __name__=="__main__":
    sys.exit(main())
