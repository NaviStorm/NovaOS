// Corps du scénario de qualification SDK, dans la seule copie de construction.
    use crate::terminal::{ERROR,OUTPUT,HEADER};
    let cases:&[(&[&str],u64)]=&[/* VECTEURS */];
    for &(words,expected) in cases {
        let mut data=[0u8;192];
        data[HEADER]=words.len() as u8;
        let mut end=HEADER+1;
        for word in words {
            data[end..end+2].copy_from_slice(&(word.len() as u16).to_le_bytes());
            end+=2;
            data[end..end+word.len()].copy_from_slice(word.as_bytes());
            end+=word.len();
        }
        data[32..40].copy_from_slice(&((end-HEADER) as u64).to_le_bytes());
        let child={
            let _gate=Guard::new();
            let child=spawn::launch_thread_witness(&data[..end]).expect("ELF tee");
            with(|s| s.processes.get_mut(child.0).terminal=OUTPUT|ERROR);
            child
        };
        assert_eq!(child.join(),Outcome::Exit(expected));
        hal::log(format_args!("TEE-ELF {:?} statut={expected} ports=OUTPUT|ERROR",words));
    }
