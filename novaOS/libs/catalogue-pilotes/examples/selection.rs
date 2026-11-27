#[path = "support/banc.rs"]
mod banc;
use novaos_catalogue::Catalog;
fn main() {
    let p = banc::policy();
    let mut c = Catalog::new(&p);
    let id = c
        .admit(banc::RAW.as_bytes(), banc::SIG, &banc::objects())
        .expect("vecteur de laboratoire");
    for isa in ["aarch64", "x86_64"] {
        let plan = c
            .select(&banc::target(isa), Some(id))
            .expect("plan du banc");
        println!(
            "{} {} {:?} {:?}",
            plan.driver(),
            plan.architecture(),
            plan.version(),
            plan.capabilities()
        );
    }
}
