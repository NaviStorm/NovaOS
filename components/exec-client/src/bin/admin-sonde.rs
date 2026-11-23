//! Témoin R1 de qualification, pas une commande d'administration distribuée.
#![no_std]
#![no_main]
use nova_cli::runtime as rt;
use nova_exec_client::admin::{AdministrativeLaunch, Conversation};
fn perform() -> rt::Result<()> {
    let launch = AdministrativeLaunch::current()?;
    if let [role @ ("--factor-writer" | "--factor-reader"), factor] = launch.streams.argv.as_slice() {
        return factor_race(&launch, *role == "--factor-writer", factor, false);
    }
    if let [role @ ("--membership-writer" | "--membership-reader")] = launch.streams.argv.as_slice() {
        return factor_race(&launch, *role == "--membership-writer", "", true);
    }
    if launch.streams.argv.as_slice()==["--prepare-group"] {
        use nova_exec_client::admin_cli::{self,Program};
        let action=admin_cli::parse(Program::Groupadd,&["equipe-r2","--prepare"]).map_err(|_|rt::BAD_ARGUMENT)?;
        let mut conversation=Conversation::new(&launch);
        let output=admin_cli::execute(Program::Groupadd,&action,launch.streams.french,|q|conversation.exchange(q)).map_err(|e|e.native)?;
        return launch.streams.write_stdout(output.as_bytes());
    }
    if launch.streams.argv.as_slice()==["--r2-refus"] {return r2_refus(&launch,false);}
    if launch.streams.argv.as_slice()==["--r2-securite"] {return r2_refus(&launch,true);}
    let mut request = [0; 128];
    request[..8].copy_from_slice(b"NXACMD01");
    let mut repeats = 1;
    let mut mode = 0;
    let mut expected_status = 0;
    let mut producer_mutation=false;
    match launch.streams.argv.as_slice() {
        [] | ["--self"] => request[8]=1,
        ["--lost-reply"] => {request[8]=1;mode=1;},
        ["--concurrent-writer"] => {request[8]=1;mode=2;},
        ["--concurrent-reader"] => {request[8]=1;mode=3;},
        [flag @ ("--last-admin-writer"|"--last-admin-reader")] => {
            request[8]=18;request[32]=2;request[33]=5;request[40..45].copy_from_slice(b"alice");
            mode=if *flag=="--last-admin-writer" {2}else{3};expected_status=rt::DENIED;
        },
        ["--confirm-writer", operation, digest] => {
            use nova_exec_client::admin_cli::{self, Action, Program};
            let Action::Confirm{request:id,digest:seal,..}=admin_cli::parse(Program::Groupadd,
                &["equipe-r2","--request",operation,"--confirm",digest]).map_err(|_|rt::BAD_ARGUMENT)?
                else {return Err(rt::BAD_ARGUMENT);};
            request[8]=32;request[16..32].copy_from_slice(&id);request[32..64].copy_from_slice(&seal);mode=2;
        },
        ["--last-admin-reader-with-mutation"] => {
            request[8]=18;request[32]=2;request[33]=5;request[40..45].copy_from_slice(b"alice");
            mode=3;expected_status=rt::DENIED;producer_mutation=true;
        },
        ["--expire"] => {request[8]=1;mode=4;},
        ["--denied-same-channel"] => {request[8]=1;mode=5;},
        ["--replay"] => {request[8]=1;mode=6;},
        ["--invalid-frame"] => {request[8]=1;mode=7;},
        ["--forged-context"] => {request[8]=1;mode=8;},
        ["--forged-meta"] => {request[8]=1;mode=9;},
        ["--add-self", user, group] => {
            request[8]=21;
            for (offset,name) in [(32,*user),(72,*group)] {
                if name.is_empty() || name.len()>32 {return Err(rt::BAD_ARGUMENT);}
                request[offset]=2;request[offset+1]=name.len() as u8;
                request[offset+8..offset+8+name.len()].copy_from_slice(name.as_bytes());
            }
        },
        ["--twice"] => {request[8]=1;repeats=2;},
        ["--user", name] | ["--group", name] => {
            if name.is_empty() || name.len()>32 {return Err(rt::BAD_ARGUMENT);}
            request[8]=if launch.streams.argv[0]=="--user" {2}else{3};
            request[32]=2;request[33]=name.len() as u8;
            request[40..40+name.len()].copy_from_slice(name.as_bytes());
        },
        ["--version"] => return launch.streams.write_stdout(b"admin-sonde 0.24.0 QEMU NXADMIN1\n"),
        ["--help"] | ["man"] => return launch.streams.write_stdout(if launch.streams.french {
            b"admin-sonde [--self|--twice|--user NOM|--group NOM]\nSonde QEMU. Statuts natifs ; mutations de qualification avec confirmation explicite.\n"
        } else {b"admin-sonde [--self|--twice|--user NAME|--group NAME]\nQEMU probe. Native statuses; qualification mutations require explicit confirmation.\n"}),
        _ => return Err(rt::BAD_ARGUMENT),
    }
    if mode>=5 {return refusals(&launch,&request,mode);}
    if matches!(mode, 1..=3) {
        return transport_probe(&launch, &request, mode, expected_status, producer_mutation);
    }
    let mut conversation = Conversation::new(&launch);
    for _ in 0..repeats {
        let reply = conversation.exchange(&request)?;
        // Vue identité de 64 octets, distincte du rendu terminal. La trame
        // complète de 128 octets a été validée avant cet extrait.
        launch.streams.write_stdout(&reply[..64])?;
        let status = u64::from_le_bytes(reply[..8].try_into().unwrap());
        if status != 0 {
            return Err(status);
        }
    }
    if mode == 4 {
        // Un bail inchangé expire dans l'invité ; aucune horloge ou gestion de
        // processus supplémentaire n'est accordée à cet ELF.
        for _ in 0..8 {
            match rt::receive_for(launch.endpoint(), 15000) {
                Err(rt::TIMED_OUT) => (),
                Err(rt::REVOKED) => {
                    if !matches!(
                        rt::receive_for(launch.endpoint(), 0),
                        Err(rt::REVOKED | rt::DENIED)
                    ) {
                        return Err(201);
                    }
                    return Err(rt::REVOKED);
                }
                _ => return Err(202),
            }
        }
        return Err(203);
    }
    Ok(())
}
fn refusals(launch: &AdministrativeLaunch<'_>, request: &[u8;128], mode:u8) -> rt::Result<()> {
    use nova_exec_client::admin_wire::{self, Direction, Inbox};
    let key=launch.endpoint();
    let mut conversation=Conversation::new(launch);
    if mode==5 {
        let first=conversation.exchange(request)?;
        if first[..8]!=[0;8] {return Err(212);}
        let mut other=*request;other[8]=2;other[32]=2;other[33]=3;other[40..43].copy_from_slice(b"bob");
        let denied=conversation.exchange(&other)?;
        if denied[..8]!=rt::DENIED.to_le_bytes() || denied[8..]!=[0;120] {return Err(213);}
        let again=conversation.exchange(request)?;
        if first!=again {return Err(214);}
        let mut result=[0;64];result[8..16].copy_from_slice(&rt::DENIED.to_le_bytes());
        return launch.streams.write_stdout(&result);
    }
    if mode==9 {
        let status=rt::call(rt::IPC_NOTIFY,key,0,0x4e5841444d455441).status;
        if status!=rt::DENIED {return Err(215);}
        let mut answer=[0;64];answer[..8].copy_from_slice(&status.to_le_bytes());
        launch.streams.write_stdout(&answer)?;return Err(status);
    }
    if mode==6 {
        let first=conversation.exchange(request)?;
        if first[..8]!=[0;8] {return Err(216);}
    }
    let mut command=*request;
    if mode==8 {command[..8].copy_from_slice(b"NXACTX01");}
    let mut frame=admin_wire::frame(1,Direction::Request,&command,0).map_err(|_|217u64)?;
    if mode==7 {frame[23]=1;}
    rt::send_for(key,1,&frame,5000)?;
    if mode==8 {
        let ack=rt::receive_for(key,5000)?;
        if ack.kind!=1 || ack.tag!=1 || ack.len!=8 || ack.payload[..8]!=[0;8] {return Err(218);}
        let second=admin_wire::frame(1,Direction::Request,&command,96).map_err(|_|219u64)?;
        rt::send_for(key,1,&second,5000)?;
    }
    let mut inbox=Inbox::new();let mut answer=[0;128];
    for _ in 0..2 {
        let m=rt::receive_for(key,5000)?;
        if m.kind!=1 || m.len!=128 || m.tag!=1 || m.handle!=0 || m.aux!=0 {return Err(220);}
        if inbox.accept(&m.payload,Direction::Response).map_err(|_|221u64)? {
            answer.copy_from_slice(inbox.message().map_err(|_|222u64)?.1);
        }
    }
    if answer[..8]!=rt::BAD_ARGUMENT.to_le_bytes() || answer[8..]!=[0;120] {return Err(223);}
    launch.streams.write_stdout(&answer[..64])?;
    Err(rt::BAD_ARGUMENT)
}
/// Même processus et même canal : chaque opération doit rester contrôlée par
/// le serveur. Les refus n'autorisent ni une autre identité ni une mutation.
fn r2_refus(launch:&AdministrativeLaunch<'_>, security:bool)->rt::Result<()> {
    use nova_exec_client::admin_wire::{self,Direction,Inbox};
    let mut c=Conversation::new(launch);
    let mut own=[0;128];own[..8].copy_from_slice(b"NXACMD01");own[8]=1;
    let original=c.exchange(&own)?;
    if original[..8]!=[0;8] || original[58]!=0 {return Err(224);}
    launch.streams.write_stdout(&original[..64])?;
    fn named(q:&mut [u8],name:&[u8]) {q[0]=2;q[1]=name.len() as u8;q[8..8+name.len()].copy_from_slice(name);}
    let mut other=own;other[8]=2;named(&mut other[32..72],b"alice");
    let mut promote=own;promote[8]=21;named(&mut promote[32..72],b"bob");named(&mut promote[72..112],b"administrateurs");
    for q in [&other,&promote] {
        let denied=c.exchange(q)?;
        if denied[..8]!=rt::DENIED.to_le_bytes() || denied[8..]!=[0;120] {return Err(225);}
        launch.streams.write_stdout(&denied[..64])?;
    }
    let again=c.exchange(&own)?;
    if again!=original {return Err(226);}
    launch.streams.write_stdout(&again[..64])?;
    let status=rt::call(rt::IPC_NOTIFY,launch.endpoint(),0,0x4e5841444d455441).status;
    if status!=rt::DENIED {return Err(227);}
    let mut denied=[0;64];denied[..8].copy_from_slice(&status.to_le_bytes());
    launch.streams.write_stdout(&denied)?;
    // Le client normal refuse ce contexte au décodage. Le témoin l'envoie
    // directement pour exercer aussi le refus du vérificateur, puis son rejeu.
    let raw=|id:u64,q:&[u8;128],replay:bool|->rt::Result<[u8;128]> {
        for offset in if replay {&[0][..]} else {&[0,96][..]} {
            let f=admin_wire::frame(id,Direction::Request,q,*offset).map_err(|_|228u64)?;
            rt::send_for(launch.endpoint(),id,&f,5000)?;
            if !replay && *offset==0 {
                let ack=rt::receive_for(launch.endpoint(),5000)?;
                if ack.kind!=1 || ack.tag!=id || ack.len!=8 || ack.handle!=0 || ack.aux!=0 || ack.payload[..8]!=[0;8] {return Err(229);}
            }
        }
        let mut inbox=Inbox::new();let mut out=[0;128];let mut complete=false;
        for _ in 0..2 {
            let m=rt::receive_for(launch.endpoint(),5000)?;
            if m.kind!=1 || m.tag!=id || m.len!=128 || m.handle!=0 || m.aux!=0 {return Err(230);}
            complete=inbox.accept(&m.payload,Direction::Response).map_err(|_|231u64)?;
            if complete {out.copy_from_slice(inbox.message().map_err(|_|232u64)?.1);}
        }
        if !complete {return Err(233);}
        Ok(out)
    };
    let mut forged=own;forged[..8].copy_from_slice(b"NXACTX01");
    for replay in [false,true] {
        let refused=raw(5,&forged,replay)?;
        if refused[..8]!=rt::BAD_ARGUMENT.to_le_bytes() || refused[8..]!=[0;120] {return Err(234);}
        launch.streams.write_stdout(&refused[..64])?;
    }
    let final_view=raw(6,&own,false)?;
    if final_view!=original {return Err(235);}
    launch.streams.write_stdout(&final_view[..64])?;
    if !security {return Ok(());}
    // Requêtes directes, sans le décodeur du client de commandes. Le serveur
    // doit refuser avant mutation ; aucune capacité nouvelle dans ce témoin.
    let mut sequence=7;
    let mut check=|q:&[u8;128],expected:u64|->rt::Result<()> {
        let answer=raw(sequence,q,false)?;sequence+=1;
        if answer[..8]!=expected.to_le_bytes() || answer[8..]!=[0;120] {return Err(236);}
        launch.streams.write_stdout(&answer[..64])
    };
    let mut confirm=own;confirm[8]=32;confirm[16..32].fill(0x71);confirm[32..64].fill(0x72);
    check(&confirm,rt::DENIED)?;
    let mut outcome=own;outcome[8]=33;outcome[16..32].fill(0x71);
    check(&outcome,rt::DENIED)?;
    // Format réel APPEND du port privé de stockage : tables 0/1/2. Bob ne
    // possède pas ce port. Le canal administratif refuse ces trames, puis le
    // noyau refuse l'envoi direct sans handle. Il ne s'agit pas d'un refus VFS
    // après ouverture : aucun droit d'ouverture n'a été attribué à cet ELF.
    for table in 0..3 {
        let mut append=[0;128];append[0]=2;append[1]=table;
        append[8..24].fill(0x73);append[28]=1;append[32]=0x74;
        check(&append,rt::BAD_ARGUMENT)?;
        let status=rt::send_for(0,1,&append,0).err().ok_or(238u64)?;
        if status!=rt::DENIED {return Err(status);}
        let mut denied=[0;64];denied[..8].copy_from_slice(&status.to_le_bytes());
        launch.streams.write_stdout(&denied)?;
    }
    // Cibles distinctes : les noms ne confèrent jamais les droits du titulaire.
    for (code,name) in [(18,b"alice".as_slice()),(19,b"bob".as_slice()),(22,b"alice".as_slice())] {
        let mut q=own;q[8]=code;named(&mut q[32..72],name);
        if code==22 {named(&mut q[72..112],b"base");}
        check(&q,rt::DENIED)?;
    }
    let unchanged=raw(sequence,&own,false)?;
    if unchanged!=original {return Err(239);}
    launch.streams.write_stdout(&unchanged[..64])
}
fn transport_probe(
    launch: &AdministrativeLaunch<'_>,
    request: &[u8; 128],
    mode: u8,
    expected_status: u64,
    producer_mutation: bool,
) -> rt::Result<()> {
    use nova_exec_client::admin_wire::{self, Direction, Inbox};
    let key = launch.endpoint();
    let mut inbox = Inbox::new();
    rt::send_for(
        key,
        1,
        &admin_wire::frame(1, Direction::Request, request, 0).map_err(|_| rt::BAD_ARGUMENT)?,
        5000,
    )?;
    let ack = rt::receive_for(key, 5000)?;
    if ack.kind != 1 || ack.tag != 1 || ack.len != 8 || ack.payload[..8] != [0; 8] {
        return Err(204);
    }
    // Neuf trames dépassent le pipe borné à huit trames. Le producteur
    // reste dans sa requête R1 partielle jusqu'à la première réception du
    // lecteur, effectuée seulement après son propre ACK R1.
    if mode == 2 {
        launch.streams.write_stdout(&[b'R'; 9 * 128])?;
    }
    if mode == 3 {
        let mut ready = [0; 128];
        for _ in 0..9 {
            if launch.streams.read_stdin(&mut ready)? != 128 || ready != [b'R'; 128] {
                return Err(205);
            }
        }
    }
    rt::send_for(
        key,
        1,
        &admin_wire::frame(1, Direction::Request, request, 96).map_err(|_| rt::BAD_ARGUMENT)?,
        5000,
    )?;
    let mut answer = [0; 128];
    for frame in 0..2 {
        let m = rt::receive_for(key, 5000)?;
        if m.kind != 1 || m.len != 128 || m.tag != 1 || m.handle != 0 || m.aux != 0 {
            return Err(206);
        }
        let complete = inbox
            .accept(&m.payload, Direction::Response)
            .map_err(|_| 207u64)?;
        if mode == 1 && frame == 0 {
            if complete {
                return Err(208);
            }
            // Première partie reçue : le registre a décidé. Perdre le suffixe
            // n'autorise aucune répétition de cette requête.
            rt::value(rt::call(rt::IPC_CLOSE, key, 0, 0))?;
            answer[..8].copy_from_slice(&22u64.to_le_bytes());
            launch.streams.write_stdout(&answer[..64])?;
            return Err(22);
        }
        if complete {
            answer.copy_from_slice(inbox.message().map_err(|_| 209u64)?.1);
        }
    }
    let status = u64::from_le_bytes(answer[..8].try_into().unwrap());
    if status != expected_status {
        return Err(if status==0 {rt::REFUSED}else{status});
    }
    if mode == 3 {
        let mut producer = [0; 128];
        if launch.streams.read_stdin(&mut producer)? != 64 {
            return Err(210);
        }
        if producer_mutation {
            if producer[..8]!=[0;8] || producer[8..24]==[0;16]
                || producer[24..32]!=24u64.to_le_bytes() || producer[32]!=1 || producer[33..64]!=[0;31] {
                return Err(211);
            }
            launch.streams.write_stdout(&producer[..64])?;
        } else if producer[..64] != answer[..64] {
            return Err(211);
        }
    }
    launch.streams.write_stdout(&answer[..64])?;
    rt::value(rt::call(rt::IPC_CLOSE, key, 0, 0))?;
    Ok(())
}
/// Deux créations distinctes du même facteur, avec requêtes PREPARE en vol
/// simultanément. Le pipe synchronise les deux premiers acquittements IPC.
/// Aucun changement de capacité, de table d'intentions ou d'oracle serveur.
fn factor_race(launch:&AdministrativeLaunch<'_>, writer:bool, factor:&str, removal:bool)->rt::Result<()> {
    use nova_exec_client::{admin_wire::{self,Direction,Inbox},admin_command};
    if !removal && (factor.len()!=32 || !factor.is_ascii()) {return Err(rt::BAD_ARGUMENT);}
    let mut q=[0;128];q[..8].copy_from_slice(b"NXACMD01");q[8]=if removal{22}else{16};
    for (offset,name) in [(32,if removal {b"alice".as_slice()}else if writer {b"carol".as_slice()}else{b"caroline".as_slice()}),(72,b"base")] {
        q[offset]=2;q[offset+1]=name.len() as u8;q[offset+8..offset+8+name.len()].copy_from_slice(name);
    }
    if !removal {for i in 0..16 {q[112+i]=u8::from_str_radix(&factor[i*2..i*2+2],16).map_err(|_|rt::BAD_ARGUMENT)?;}}
    admin_command::decode(&q).map_err(|_|rt::BAD_ARGUMENT)?;
    let exchange=|tag:u64,q:&[u8;128],barrier:bool|->rt::Result<[u8;128]> {
        let key=launch.endpoint();
        rt::send_for(key,tag,&admin_wire::frame(tag,Direction::Request,q,0).map_err(|_|rt::BAD_ARGUMENT)?,5000)?;
        let ack=rt::receive_for(key,5000)?;
        if ack.kind!=1 || ack.tag!=tag || ack.len!=8 || ack.handle!=0 || ack.aux!=0 || ack.payload[..8]!=[0;8] {return Err(241);}
        if barrier {
            if writer {launch.streams.write_stdout(&[b'F';9*128])?;}
            else {let mut b=[0;128];for _ in 0..9 {if launch.streams.read_stdin(&mut b)?!=128 || b!=[b'F';128] {return Err(242);}}}
        }
        rt::send_for(key,tag,&admin_wire::frame(tag,Direction::Request,q,96).map_err(|_|rt::BAD_ARGUMENT)?,5000)?;
        let mut inbox=Inbox::new();let mut result=None;
        for _ in 0..2 {
            let m=rt::receive_for(key,5000)?;
            if m.kind!=1 || m.tag!=tag || m.len!=128 || m.handle!=0 || m.aux!=0 {return Err(243);}
            if inbox.accept(&m.payload,Direction::Response).map_err(|_|244u64)? {
                result=Some(inbox.message().map_err(|_|245u64)?.1.try_into().map_err(|_|246u64)?);
            }
        }
        result.ok_or(247)
    };
    let prepared=exchange(1,&q,true)?;
    let mut result=[0;96];result[80..96].copy_from_slice(&prepared[8..24]);result[..8].copy_from_slice(&prepared[..8]);
    if prepared[..8]==[0;8] {
        let mut confirm=[0;128];confirm[..8].copy_from_slice(b"NXACMD01");confirm[8]=32;
        confirm[16..32].copy_from_slice(&prepared[8..24]);confirm[32..64].copy_from_slice(&prepared[24..56]);
        let answer=exchange(2,&confirm,false)?;
        result[8..16].copy_from_slice(&answer[..8]);result[16..80].copy_from_slice(&answer[..64]);
    }else {result[8..16].copy_from_slice(&u64::MAX.to_le_bytes());}
    let length=if removal{96}else{80};
    if !writer {
        let mut producer=[0;128];if launch.streams.read_stdin(&mut producer)?!=length {return Err(248);}
        launch.streams.write_stdout(&producer[..length])?;
    }
    launch.streams.write_stdout(&result[..length])?;
    rt::value(rt::call(rt::IPC_CLOSE,launch.endpoint(),0,0))?;
    Ok(())
}
#[unsafe(no_mangle)]
extern "C" fn run() -> ! {
    rt::exit(perform().err().unwrap_or(0))
}
