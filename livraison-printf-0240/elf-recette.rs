// Included exclusively in the private sdk-example test controller.
fn exercice_elf() {
    use crate::terminal::{HEADER,OUTPUT,ERROR};
    const ELF:&[u8]=include_bytes!(env!("PRINTF_ELF"));
    let cases:&[(&[&[u8]],u8,u64)]=&[
        (&[b"%2$s/%1$04x",b"15",b"Nova"],OUTPUT,0),
        (&[b"%+.2f|%a",b"1.125",b"0x1.8p-2"],OUTPUT,0),
        (&[b"%b",br"a\cb"],OUTPUT,0),
        (&[b"%d",b"9x"],OUTPUT|ERROR,1),
        (&[b"--help"],OUTPUT,0),(&[b"--version"],OUTPUT,0),
        (&[b"--help",b"x"],OUTPUT|ERROR,0),
        (&[b"%b",br"\0"],OUTPUT|ERROR,1),(&[b"%b",br"\t"],OUTPUT|ERROR,1),
        (&[b"%c","é".as_bytes()],OUTPUT|ERROR,1),
        (&[b"x"],ERROR,1),(&[b""],0,0),(&[b"debut%Z"],OUTPUT|ERROR,1),
    ];
    for (index,&(args,ports,status)) in cases.iter().enumerate(){
        let mut data=[0u8;512];data[HEADER]=args.len() as u8;let mut end=HEADER+1;
        for arg in args{data[end..end+2].copy_from_slice(&(arg.len() as u16).to_le_bytes());
            end+=2;data[end..end+arg.len()].copy_from_slice(arg);end+=arg.len();}
        data[32..40].copy_from_slice(&((end-HEADER) as u64).to_le_bytes());
        hal::log(format_args!("PRINTF ELF debut={index} ports={ports}"));
        let child={
            let _guard=Guard::new();
            let child=create_image(ELF,&data[..end],None,None).unwrap();
            with(|s|s.processes.get_mut(child.0).terminal=ports);child
        };
        let outcome=child.join();
        assert_eq!(outcome,Outcome::Exit(status));
        hal::log(format_args!("PRINTF ELF fin={index} statut={status}"));
    }
    let hz=crate::arch::timer::counter_hz();
    for mode in 1u64..=4 {
        for pass in 0..6 {
            let start=crate::arch::timer::counter();
            let probe=spawn::launch_thread_witness(&mode.to_le_bytes()).unwrap();
            assert_eq!(probe.join(),Outcome::Exit(0));
            let elapsed=crate::arch::timer::counter()-start;
            hal::log(format_args!("PRINTF PERF mode={mode} pass={pass} cycles={elapsed} hz={hz} appels=128"));
        }
    }
}
