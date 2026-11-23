#![cfg(feature="administration-lab")]
use novaos_recovery::{membership_effectors::*, hash};
fn query()->VfsClosure {VfsClosure::new([1;16],[2;16],[3;16],4,5,[6;32]).unwrap()}
fn reply(q:&VfsClosure)->[u8;128] {
    let mut r=[0;128];r[8..16].copy_from_slice(b"NXMGACK1");r[16..48].copy_from_slice(&hash(q.bytes()));
    r[48..80].copy_from_slice(&[6;32]);r[80..88].copy_from_slice(&2u64.to_le_bytes());r[104..112].copy_from_slice(&3u64.to_le_bytes());r
}
#[test] fn native_vfs_fact_is_bound_to_exact_request_and_live_instance() {
    let q=query();let r=reply(&q);let facts=q.verify(&r).unwrap();
    assert_eq!((facts.removed(),facts.retained(),facts.cancelled_prepare(),facts.notified()),(2,3,None,false));
    for i in (0..80).chain(96..104).chain(120..128) {
        let mut a=r;a[i]^=1;assert!(q.verify(&a).is_err(),"offset {i}");
    }
    for n in 0..128 {assert!(q.verify(&r[..n]).is_err());}
    assert!(q.verify(&[0;129]).is_err());
    let other=VfsClosure::new([9;16],[2;16],[3;16],4,5,[6;32]).unwrap();assert!(other.verify(&r).is_err());
}
#[test] fn pending_wake_unknown_status_and_impossible_inventory_are_not_success() {
    let q=query();let mut r=reply(&q);
    r[0]=7;assert!(q.verify(&r).is_err());r[0]=0;
    r[112]=1;assert!(q.verify(&r).is_err());r[88]=33;
    let facts=q.verify(&r).unwrap();assert_eq!(facts.cancelled_prepare(),Some(33));assert!(facts.notified());
    r[80]=0;assert!(q.verify(&r).is_err());r[80]=13;assert!(q.verify(&r).is_err());
    r[80]=2;r[104]=13;assert!(q.verify(&r).is_err());r[104]=3;r[112]=2;assert!(q.verify(&r).is_err());
}
#[test] fn nonzero_identity_and_position_with_bounded_membership_revision() {
    for (op,subject,group,revision,position,instance) in [
        ([0;16],[2;16],[3;16],4,5,[6;32]),([1;16],[0;16],[3;16],4,5,[6;32]),
        ([1;16],[2;16],[0;16],4,5,[6;32]),([1;16],[2;16],[3;16],1<<63,5,[6;32]),
        ([1;16],[2;16],[3;16],4,0,[6;32]),([1;16],[2;16],[3;16],4,1<<63,[6;32]),([1;16],[2;16],[3;16],4,5,[0;32])] {
        assert!(VfsClosure::new(op,subject,group,revision,position,instance).is_err());
    }
}

#[test] fn initial_membership_revision_zero_is_not_a_direct_permission() {
    let q=VfsClosure::new([1;16],[2;16],[3;16],0,5,[6;32]).unwrap();
    assert_eq!(&q.bytes()[56..64],&0u64.to_le_bytes());
    assert_eq!(&q.bytes()[40..56],&[3;16]);q.verify(&reply(&q)).unwrap();
}
