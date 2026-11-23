
#[test] fn global_incremental_control_layout() {
    let pending=core::mem::size_of::<Option<coordinator::Selective>>();
    let completed=core::mem::size_of::<Option<([u8;16],coordinator::Selective)>>();
    let inherited=2960usize;
    println!("pending={} completed={} inherited={} subtotal={}",pending,completed,inherited,inherited+pending+completed);
    assert!(inherited+pending+completed<=4096);
}
