//! Exécution de la véritable CLI. Toutes les images passent par des tubes.
use std::{
    io::Write,
    process::{Command, Output, Stdio},
    thread,
};

fn tool(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_novafs"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut pipe = child.stdin.take().unwrap();
    thread::scope(|scope| {
        scope.spawn(move || {
            let _ = pipe.write_all(input);
        });
        child.wait_with_output().unwrap()
    })
}
#[test]
fn cli_images_anonymes_et_sauvetage() {
    let help = tool(&["--help"], b"");
    assert!(help.status.success());
    assert!(help.stdout.starts_with(b"NovaFS P0"));
    for sub in ["create", "inspect", "verify", "script", "salvage"] {
        assert!(tool(&[sub, "--help"], b"").status.success())
    }
    assert!(tool(&["create", "180", "512"], b"").status.success());
    let image = tool(
        &["script", "256", "4096"],
        b"mkdir /d\ncreate /d/x\nwrite /d/x 0 0010ff\nwrite /d/x 3 ab\nverify\n",
    );
    assert!(
        image.status.success(),
        "{:?}",
        String::from_utf8_lossy(&image.stderr)
    );
    assert_eq!(image.stdout.len(), 256 * 4096);
    let diag = String::from_utf8(image.stderr).unwrap();
    let mut ids = diag.lines().next().unwrap().split_whitespace();
    let fs = ids.next().unwrap().strip_prefix("FS=").unwrap();
    let lv = ids.next().unwrap().strip_prefix("LV=").unwrap();
    let verify = ["verify", "256", "512", fs, lv];
    assert!(tool(&verify, &image.stdout).status.success());
    assert!(
        !tool(&verify, &image.stdout[..image.stdout.len() - 1])
            .status
            .success()
    );
    assert!(
        tool(&["inspect", "256", "4096"], &image.stdout)
            .status
            .success()
    );
    let mut broken = image.stdout; // Génération 5 en B, génération 4 conserve les trois premiers octets.
    broken[4096 + 64] ^= 1;
    assert!(!tool(&verify, &broken).status.success());
    assert!(!tool(&["inspect", "256", "512"], &broken).status.success());
    let extract = tool(&["salvage", "256", "512", fs, lv, "0", "/d/x"], &broken);
    assert!(extract.status.success());
    assert_eq!(extract.stdout, [0, 0x10, 0xff]);
    assert!(
        String::from_utf8(extract.stderr)
            .unwrap()
            .contains("fraîcheur indéterminée")
    );
    assert!(
        !tool(&["salvage", "256", "512", fs, lv, "1", "/d/x"], &broken)
            .status
            .success()
    );
    assert!(!tool(&["create", "180", "1024"], b"").status.success());
}
