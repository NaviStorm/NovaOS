use serde_json::Value;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    process::{Command, Output, Stdio},
};

const FS_ID: &str = "31313131313131313131313131313131";
const LV_ID: &str = "52525252525252525252525252525252";

fn command() -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_novafs"));
    cmd.args(["check", "320", "4096", FS_ID, LV_ID]);
    cmd
}

fn report(result: &Output, code: i32) -> Value {
    assert_eq!(result.status.code(), Some(code), "{result:?}");
    let object: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(object["schema"], "novafs-check-1");
    object
}

#[test]
fn sorties_alias_et_entrees_non_seekables() {
    let tmp = tempfile::tempdir().unwrap();
    let image = tmp.path().join("image");
    let alias = tmp.path().join("lien");
    let bytes = vec![0x65; 320 * 4096];
    fs::write(&image, &bytes).unwrap();
    fs::hard_link(&image, &alias).unwrap();
    for stderr in [false, true] {
        let mut cmd = command();
        let output = OpenOptions::new().append(true).open(&alias).unwrap();
        if stderr {
            cmd.stderr(output);
        } else {
            cmd.stdout(output);
        }
        let result = cmd.stdin(File::open(&image).unwrap()).output().unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty() && result.stderr.is_empty());
        assert_eq!(fs::read(&image).unwrap(), bytes);
    }
    let result = command().stdin(Stdio::piped()).output().unwrap();
    assert_eq!(report(&result, 2)["statut"], "usage");
}

#[test]
fn depot_du_rapport_sur_fichier_ouvert_en_lecture_seule() {
    use novafs::{Engine, Ids, device::FileImage};
    let mut image = tempfile::NamedTempFile::new().unwrap();
    image.as_file().set_len(320 * 4096).unwrap();
    let dev = FileImage::from_file(image.reopen().unwrap(), 320, 4096).unwrap();
    let mut fs = Engine::format(
        dev,
        Ids {
            fs: [0x31; 16],
            lv: [0x52; 16],
        },
    )
    .unwrap();
    fs.create("/temoin").unwrap();
    fs.write("/temoin", 0, b"publication").unwrap();
    drop(fs.unmount().unwrap());
    let read = || {
        command()
            .stdin(File::open(image.path()).unwrap())
            .output()
            .unwrap()
    };
    let before = fs::read(image.path()).unwrap();
    let good = report(&read(), 0);
    assert_eq!(good["statut"], "coherent");
    assert_eq!(good["paire"]["slot"], 1);
    assert!(
        good["slots"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["vue_verifiee"] == true)
    );
    assert_eq!(before, fs::read(image.path()).unwrap());
    // Le descripteur de la fixture est resté positionné à zéro (E/S moteur positionnelles).
    image.write_all(b"X").unwrap();
    let damaged = fs::read(image.path()).unwrap();
    let rejected = report(
        &command()
            .stdin(File::open(image.path()).unwrap())
            .output()
            .unwrap(),
        1,
    );
    assert_eq!(rejected["slots"][0]["vue_verifiee"], false);
    assert_eq!(rejected["slots"][1]["vue_verifiee"], true);
    assert!(rejected["paire"]["slot"].is_null());
    assert_eq!(damaged, fs::read(image.path()).unwrap());
}
