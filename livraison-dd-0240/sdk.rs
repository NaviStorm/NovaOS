use crate::terminal::{OUTPUT, ERROR, HEADER};
    for (generic, words, code) in [
        (false, &["--help"][..], 0),
        (false, &["--help","--","nsh-doc=fr"], 0),
        (false, &["--version"], 0),
        (false, &["count=0"], 5),
        (true, &["count=0","status=none"], 0),
        (true, &["count=1","status=none"], 1),
        (true, &["of=inaccessible","status=none"], 1),
    ] {
        let mut data=[0u8;192];
        let header=if generic {64} else {HEADER};
        data[header]=words.len() as u8;
        let mut end=header+1;
        for word in words {
            data[end..end+2].copy_from_slice(&(word.len() as u16).to_le_bytes());
            end+=2;
            data[end..end+word.len()].copy_from_slice(word.as_bytes());
            end+=word.len();
        }
        let index=if generic {data[..8].copy_from_slice(b"NXEXEC01");48} else {32};
        data[index..index+8].copy_from_slice(&((end-header) as u64).to_le_bytes());
        let child={
            let _guard=Guard::new();
            let app=spawn::launch_thread_witness(&data[..end]).expect("dd ELF");
            if !generic {with(|s|s.processes.get_mut(app.0).terminal=OUTPUT|ERROR);}
            app
        };
        assert_eq!(child.join(),Outcome::Exit(code));
        hal::log(format_args!("DD-ELF argv={words:?} rc={code}"));
    }
