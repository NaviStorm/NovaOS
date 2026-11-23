// Harnais natif : coordinateur réel, RPC et stockage simulés explicitement.
// Ne démontre ni l'authentification des canaux ni un acquittement invité.
use super::*;
fn setup()->Selective {
    crate::sim::reset();
    Selective{binding:crate::sim::binding(),principal:[8;16],epoch:1,
        instances:[[4;32],[5;32]],reserve_sent:false,position:0,closed:false,exec:false,ack_sent:0}
}
fn reserved()->Selective {
    let mut s=setup();s.reserve(&crate::sim::head(false)).unwrap();s
}
fn closed()->Selective {
    let mut s=reserved();s.exec_verified();s.close(&mut Storage::new()).unwrap();
    crate::sim::publish();s
}
#[test] fn no_selection_before_authenticated_reservation() {
    let s=setup();assert_eq!(s.selection(&mut[0;128]),Err(22));assert_eq!(crate::sim::calls(54),0);
}
#[test] fn reservation_response_lost_is_queried_without_resubmission() {
    let mut s=setup();crate::sim::lose(54,true);assert_eq!(s.reserve(&crate::sim::head(false)),Err(22));
    s.reserve(&crate::sim::head(false)).unwrap();assert_eq!(crate::sim::calls(54),1);assert!(s.position>0);
}
#[test] fn reservation_not_accepted_cannot_be_invented_or_replayed() {
    let mut s=setup();crate::sim::lose(54,false);assert_eq!(s.reserve(&crate::sim::head(false)),Err(22));
    assert!(s.reserve(&crate::sim::head(false)).is_err());assert_eq!(crate::sim::calls(54),1);
}
#[test] fn closure_requires_actual_exec_inspection() {
    let mut s=reserved();let mut store=Storage::new();assert_eq!(s.close(&mut store),Err(22));assert_eq!(store.calls,0);
}
#[test] fn local_cancelled_write_without_individual_admission_is_uncertain() {
    let mut s=reserved();s.exec_verified();let mut store=Storage::new();store.cancelled=37;
    assert_eq!(s.close(&mut store),Err(22));crate::sim::publish();assert_eq!(s.finish_head(&crate::sim::p9()),Err(22));
    assert_eq!(crate::sim::calls(56),0);
}
#[test] fn timeout_is_not_an_acknowledgement() {
    let mut s=reserved();s.exec_verified();let mut store=Storage::new();store.error=Some(8);
    assert_eq!(s.close(&mut store),Err(8));assert!(!s.closed);assert_eq!(crate::sim::calls(56),0);
}
#[test] fn effectors_are_acknowledged_only_after_matching_p9_and_authority_heads() {
    let mut s=closed();let mut wrong=crate::sim::p9();wrong.digest[0]^=1;
    assert_eq!(s.finish_head(&wrong),Err(22));assert_eq!(crate::sim::calls(56),0);
    s.finish_head(&crate::sim::p9()).unwrap();assert_eq!(crate::sim::calls(56),3);
}
#[test] fn lost_durable_ack_is_reconciled_without_second_ack() {
    let mut s=closed();crate::sim::lose(56,true);assert_eq!(s.finish_head(&crate::sim::p9()),Err(22));
    s.finish_head(&crate::sim::p9()).unwrap();assert_eq!(crate::sim::calls(56),3);
    s.finish_head(&crate::sim::p9()).unwrap();assert_eq!(crate::sim::calls(56),3);
}
#[test] fn absent_ack_keeps_withdrawal_uncertain() {
    let mut s=closed();crate::sim::lose(56,false);assert_eq!(s.finish_head(&crate::sim::p9()),Err(22));
    assert_eq!(s.finish_head(&crate::sim::p9()),Err(22));assert_eq!(crate::sim::calls(56),1);
}
#[test] fn substituted_signed_receipt_is_not_terminal() {
    let mut s=closed();crate::sim::substitute();assert_eq!(s.finish_head(&crate::sim::p9()),Err(12));
    assert_eq!(crate::sim::calls(56),0);
}
#[test] fn measured_selective_state_stays_bounded() {
    println!("Selective={} bytes",core::mem::size_of::<Selective>());assert!(core::mem::size_of::<Selective>()<=512);
}

#[test] fn private_bootstrap_reads_only_designated_instance_ranges() {
    for first in [23,27] {
        let got=identity_from(first,|word| {assert!((first..first+4).contains(&word));word as u64}).unwrap();
        for i in 0..4 {assert_eq!(&got[i*8..i*8+8],&((first+i) as u64).to_le_bytes());}
    }
    for first in [0,19,20,22,24,26,28,31,usize::MAX] {
        assert_eq!(identity_from(first,|_|panic!("lecture interdite")),Err(5));
    }
    assert_eq!(identity_from(23,|_|0),Err(5));
}

#[test] fn closure_trace_preserves_text_and_counts() {
 assert_eq!(closure_marker(1,2).unwrap(),*b"NARM VFS fermeture selective acquittee retires=01 conserves=02");
 assert_eq!(closure_marker(0,12).unwrap(),*b"NARM VFS fermeture selective acquittee retires=00 conserves=12");
 assert_eq!(closure_marker(100,0),Err(rt::REFUSED));
}

#[test]
fn terminal_release_requires_verified_durable_receipt() {
    let mut selective = closed();
    let mut storage = Storage::new();
    let head = crate::sim::p9();
    crate::sim::lose(56, true);
    assert_eq!(selective.finish_and_release(&head, &mut storage), Err(22));
    assert_eq!(storage.calls, 0);
    selective.finish_and_release(&head, &mut storage).unwrap();
    assert_eq!(crate::sim::calls(56), 3);
    assert_eq!(storage.calls, 1);
    crate::sim::substitute();
    assert_eq!(selective.finish_and_release(&head, &mut storage), Err(rt::REFUSED));
    assert_eq!(storage.calls, 1);
}

#[test]
fn nominal_path_uses_two_signed_queries_for_three_distinct_effectors() {
    let mut selective = closed();
    let queries = crate::sim::calls(59);
    let mut storage = Storage::new();
    selective.finish_and_release(&crate::sim::p9(), &mut storage).unwrap();
    assert_eq!(crate::sim::calls(56), 3);
    assert_eq!(crate::sim::calls(59) - queries, 2);
    assert_eq!(storage.calls, 1);
}
