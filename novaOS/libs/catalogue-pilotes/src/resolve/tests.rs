use super::*;
use alloc::{vec, vec::Vec};
fn need(id: &str) -> Dependency {
    Dependency {
        id: id.into(),
        kind: Kind::Service,
        min: [1, 0, 0],
        before: [2, 0, 0],
        sha256: [id.as_bytes()[0]; 32],
        required: true,
    }
}
fn node(id: &str, children: &[&str]) -> Resource {
    Resource {
        id: id.into(),
        kind: Kind::Service,
        version: [1, 0, 0],
        digest: need(id).sha256,
        record_digest: [8; 32],
        dependencies: children.iter().map(|c| need(c)).collect(),
    }
}
#[test]
fn cycle_et_diamant() {
    let root = [need("a")];
    let cycle = [node("a", &["b"]), node("b", &["a"])];
    assert_eq!(resolve(&root, &cycle), Err(Error::Cycle));
    let mut graph = vec![
        node("a", &["c", "b"]),
        node("b", &["d"]),
        node("c", &["d"]),
        node("d", &[]),
    ];
    let got = resolve(&root, &graph).unwrap();
    graph.reverse();
    assert_eq!(got, resolve(&root, &graph).unwrap());
    assert_eq!(
        got.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c", "d"]
    );
}
#[test]
fn absences_et_version() {
    let mut dep = need("a");
    assert_eq!(
        resolve(core::slice::from_ref(&dep), &[]),
        Err(Error::Dependency)
    );
    dep.required = false;
    assert_eq!(resolve(&[dep], &[]), Ok(Vec::new()));
    let mut dep = need("a");
    dep.min = [1, 1, 0];
    assert_eq!(resolve(&[dep], &[node("a", &[])]), Err(Error::Dependency));
}
