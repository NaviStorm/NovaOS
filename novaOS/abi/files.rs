//! Messages du service fichier, transportés dans le payload IPC. Entiers little-endian.
pub type Frame = [u8; 128];
pub const VERSION: u8 = 2;
pub const MOUNT: u8 = 1;
/// Opt-in privé : réponse intermédiaire après 256 pages de 4 Kio effectivement lues.
pub const MOUNT_PROGRESS: u8 = 1;
pub const MOUNT_STEP: u64 = 256;
pub fn mount_progress(pages: u64) -> Frame {
    let mut f = frame(MOUNT, 0, 0);
    f[2] = MOUNT_PROGRESS;
    set(&mut f, 32, pages);
    f
}
pub const INFO: u8 = 2;
pub const OPEN: u8 = 3;
pub const READ: u8 = 4;
pub const WRITE: u8 = 5;
pub const SYNC: u8 = 6;
pub const CLOSE: u8 = 7;
pub const READABLE: u8 = 1;
pub const WRITABLE: u8 = 2;
pub const CREATE: u8 = 4;
pub const DISCARD: u8 = 1;
pub const START: u8 = 1;
pub const CONTINUE: u8 = 2;
/// Début de fenêtre conditionné à la révision (u64 à PAYLOAD).
pub const IF_REVISION: u8 = 3;
pub const PAYLOAD: usize = 40;
pub const FRAGMENT: usize = 128 - PAYLOAD;
/// Une lecture logique du courtier, transférée par fragments sur la même capacité.
pub const READ_WINDOW: usize = 4096;
pub const MAX_WRITE: usize = 64 * 1024;
pub const NOT_FOUND: u64 = 20;
pub const CORRUPT: u64 = 21;
pub const UNCERTAIN: u64 = 22;
pub const FULL: u64 = 23;
pub const BUSY: u64 = 24;
pub const UNSUPPORTED: u64 = 25;
pub const IO: u64 = 26;
pub fn frame(op: u8, session: u64, handle: u64) -> Frame {
    let mut f = [0; 128];
    f[0] = VERSION;
    f[1] = op;
    set(&mut f, 8, session);
    set(&mut f, 16, handle);
    f
}
pub fn word(f: &Frame, at: usize) -> u64 {
    u64::from_le_bytes(f[at..at + 8].try_into().unwrap())
}
pub fn set(f: &mut Frame, at: usize, v: u64) {
    f[at..at + 8].copy_from_slice(&v.to_le_bytes())
}
pub fn length(f: &Frame) -> usize {
    u16::from_le_bytes(f[4..6].try_into().unwrap()) as usize
}
pub fn size(f: &mut Frame, n: usize) {
    f[4..6].copy_from_slice(&(n as u16).to_le_bytes())
}

// Extension de la trame v2. La charge d'un chemin se fragmente par session.
pub const PATH_FRAGMENT: u8 = 8;
pub const DIR_OPEN: u8 = 9;
pub const LOOKUP_AT: u8 = 10;
pub const OPEN_AT: u8 = 11;
pub const READDIR: u8 = 12;
pub const MKDIR_AT: u8 = 13;
pub const RMDIR: u8 = 14;
pub const UNLINK_AT: u8 = 15;
pub const RENAME_AT: u8 = 16;
pub const STAT: u8 = 17;
pub const DIR_CLOSE: u8 = 18;
pub const CHDIR: u8 = 19;
pub const GETCWD: u8 = 20;
pub const TRAVERSE: u8 = 1;
pub const FILE_READ: u8 = 2;
pub const ENUMERATE: u8 = 4;
pub const ADD: u8 = 8;
pub const REMOVE: u8 = 16;
pub const MOVE: u8 = 32;
pub const FILE_WRITE: u8 = 64;
pub const DIR_ALL: u8 = 127;
pub const DIR_RO: u8 = TRAVERSE | FILE_READ | ENUMERATE;
pub const EXISTS: u64 = 27;
pub const NOT_EMPTY: u64 = 28;
pub const NOT_DIR: u64 = 29;
pub const IS_DIR: u64 = 30;
pub const PATH_MAX: usize = 1024;

// Délégation éphémère de la session à une paire de canaux IPC dédiée.
pub const DELEGATE: u8 = 21;
pub const CWD_VIEW: u8 = 1;
pub const DIR_VIEW: u8 = 2;
pub const FILE_VIEW: u8 = 3;
pub const CREATE_ENTRY: u8 = 4;
pub const REMOVE_ENTRY: u8 = 5;

// Vues de premier plan. Une entrée déléguée fixe son parent et son nom.
pub const STAGE: u8 = 22;
pub const GROUP: u8 = 6;
pub const SELECT_FILE: u8 = 1;
pub const SELECT_LIST: u8 = 2;
pub const SELECT_TREE: u8 = 3;
pub const SELECT_NEW: u8 = 4;
pub const SELECT_TOUCH: u8 = 5;
pub const SELECT_FROM: u8 = 6;
pub const SELECT_TO: u8 = 7;
pub const CREATE_NEW: u8 = 23;
pub const MOVE_SELECTED: u8 = 24;
pub const TOUCH_SELECTED: u8 = 25;
pub const SELECT_COPY_TREE: u8 = 8;
pub const SELECT_COPY_DEST: u8 = 9;
pub const PUBLISH_COPY: u8 = 26;

// Portées supplémentaires. REMOVE_ENTRY demeure limité à UNLINK_AT.
pub const REMOVE_EMPTY_DIR: u8 = 7;
pub const REMOVE_TREE: u8 = 8;

/// Copier dans la candidate privée via les deux handles déjà sélectionnés.
/// Aucune publication de nom. Flag 1 : texte UTF-8 du profil éditeur.
pub const COPY_RANGE: u8 = 27;
pub const TEXT_RANGE: u8 = 1;

pub const READ_PAGE: u8 = 28;
pub const PAGE_FEATURE: u64 = 1;
