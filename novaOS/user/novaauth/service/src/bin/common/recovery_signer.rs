// Client du VFS et propriétaire de son seul facteur, jamais relais de clés.
fn recovery_serve()->rt::Result<()> {
    let mut client=Client {key:SigningKey::from_bytes(SEED),seq:0};
    let mut session=[0;32];
    let mut operation=[0;16];let mut previous_session=[0;32];
    loop {
        let cmd=match rt::receive_for(rt::config(1),15000) {
            Err(rt::TIMED_OUT)=>continue,other=>other?
        };
        check(cmd.kind==1 && cmd.len==128)?;
        let mut answer=[0;128];
        let result=(|| {
            match cmd.payload[0] {
                1=>{
                    // Réauthentification explicitement demandée : fermer notre
                    // session précédente, sans assimiler retrait de dossier et
                    // fermeture de session. Aucun renouvellement implicite.
                    if session!=[0;32] {
                        let prior=client.session(4,session,[0;16])?;
                        if number(&prior)!=0 {return Err(number(&prior));}
                        previous_session=session;session=[0;32];
                    }
                    let (owner,id,_)=client.login()?;session=id;
                    answer[8..24].copy_from_slice(&owner);answer[24..56].copy_from_slice(&id);
                    if previous_session!=[0;32] {
                        let prior=client.session(3,previous_session,[0;16])?;
                        answer[56..64].copy_from_slice(&prior[..8]);
                    }
                },
                2=>{
                    let mut data=[0;128];data[0]=12;
                    data[8..24].copy_from_slice(&cmd.payload[8..24]);
                    let tag=client.seq.checked_add(1).ok_or(rt::LIMIT)?;
                    rt::send_frame(rt::config(9),tag,&data,0,false)?;
                    let mut auth=client.frame(11);auth[40..72].copy_from_slice(&session);
                    auth[72..80].copy_from_slice(&cmd.payload[8..16]);
                    let decision=client.send(&auth)?;
                    if number(&decision)!=rt::OK {return Err(number(&decision));}
                    let msg=remote(rt::config(10),tag)?;
                    if &msg.payload[120..]!=b"DIRREPLY" {return Err(rt::REFUSED);}
                    answer=msg.payload;
                },
                4|5=>{
                    if cmd.payload[0]==4 {
                        if operation!=[0;16] {return Err(rt::DENIED);}
                        operation.copy_from_slice(&random()?[..16]);previous_session=session;
                    }
                    let mut q=client.frame(if cmd.payload[0]==4 {12} else {13});
                    q[40..72].copy_from_slice(&previous_session);
                    if cmd.payload[0]==4 {q[72..80].copy_from_slice(&cmd.payload[8..16]);}
                    q[88..104].copy_from_slice(&operation);
                    answer=client.send(&q)?;
                },
                3=>{
                    let r=client.session(4,session,[0;16])?;
                    if number(&r)!=0 {return Err(number(&r));}
                    previous_session=session;session=[0;32];
                },
                _=>return Err(rt::BAD_ARGUMENT)
            }
            Ok(())
        })();
        if let Err(code)=result {answer=[0;128];answer[..8].copy_from_slice(&code.to_le_bytes());}
        rt::send_for(rt::config(0),cmd.tag,&answer,5000)?;
    }
}
#[unsafe(no_mangle)]
extern "C" fn run()->! {rt::exit(recovery_serve().err().unwrap_or(0))}
