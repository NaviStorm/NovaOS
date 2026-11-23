// Tests natifs du CODE de Directory exporté, avec simulation du seul retour IPC.
#[cfg(test)]
mod tests {
    use super::*;
    fn holder(n:u8)->Holder {Holder{subject:[n;16],session:[n;32],port:n as u64}}
    fn permission(origin:Origin,rights:u8,revision:u64)->Permission {
        Permission{origin,rights,rule_revision:9,membership_revision:revision,image_revision:3}
    }
    fn request()->[u8;128] {
        *novaos_recovery::membership_effectors::VfsClosure::new([1;16],[2;16],[3;16],5,10,[4;32]).unwrap().bytes()
    }
    fn direct()->Permission {permission(Origin::Direct,READ,0)}
    fn g()->Permission {permission(Origin::Group([3;16]),WRITE,5)}
    fn h()->Permission {permission(Origin::Group([8;16]),ALL,6)}
    fn verified(q:&[u8;128],a:&[u8;128])->novaos_recovery::membership_effectors::VfsFacts {
        let query=novaos_recovery::membership_effectors::VfsClosure::new([1;16],[2;16],[3;16],5,10,[4;32]).unwrap();
        assert_eq!(query.bytes(),q);query.verify(a).unwrap()
    }
    #[test] fn only_g_and_its_descendants_are_removed() {
        let mut d=Directory::default();let who=holder(2);
        let own=d.membership_grant(who,0,READ,direct()).unwrap();
        let gh=d.membership_grant(who,0,WRITE,g()).unwrap();
        let child=d.membership_grant(who,gh,WRITE,g()).unwrap();
        let hh=d.membership_grant(who,0,ALL,h()).unwrap();
        let bob=d.membership_grant(holder(7),0,WRITE,g()).unwrap();
        let q=request();let a=d.membership_close(&q,[4;32]).unwrap();let facts=verified(&q,&a);
        assert_eq!((facts.removed(),facts.retained()),(2,3));
        assert_eq!(d.check(who,gh,WRITE),Err(rt::REVOKED));
        assert_eq!(d.check(who,child,WRITE),Err(rt::REVOKED));
        d.check(who,own,READ).unwrap();d.check(who,hh,WRITE).unwrap();d.check(holder(7),bob,WRITE).unwrap();
        assert_eq!(d.membership_grant(who,0,WRITE,g()),Err(rt::REVOKED));
        assert_eq!(d.membership_close(&q,[4;32]).unwrap(),a);
        assert!(d.membership_grant(who,0,WRITE,permission(Origin::Group([3;16]),WRITE,11)).is_ok());
        assert_eq!(d.check(who,gh,WRITE),Err(rt::REVOKED));
    }
    #[test] fn provenance_cannot_be_replaced_at_delegation() {
        let mut d=Directory::default();let who=holder(2);let key=d.membership_grant(who,0,WRITE,g()).unwrap();
        assert_eq!(d.membership_grant(who,key,READ,direct()),Err(rt::DENIED));
        assert_eq!(d.membership_grant(holder(7),key,WRITE,g()),Err(rt::REVOKED));
        assert_eq!(d.membership_grant(who,0,ALL,g()),Err(rt::DENIED));
    }
    #[test] fn ipc_saturation_keeps_cancelled_operation_and_wakeup_for_retry() {
        let mut d=Directory::default();let who=holder(2);let key=d.membership_grant(who,0,WRITE,g()).unwrap();
        let hh=d.membership_grant(who,0,ALL,h()).unwrap();
        d.pending=Some(PendingWrite{holder:who,key,request:123});d.sleeper=Some((who,88,4));
        rt::STATUS.with(|s|s.set(rt::WOULD_BLOCK));
        let q=request();assert_eq!(d.membership_close(&q,[4;32]),Err(rt::WOULD_BLOCK));
        assert!(d.pending.is_none());assert!(d.sleeper.is_some());assert_eq!(d.check(who,key,WRITE),Err(rt::REVOKED));
        d.check(who,hh,WRITE).unwrap();
        rt::STATUS.with(|s|s.set(rt::OK));let answer=d.membership_close(&q,[4;32]).unwrap();
        let facts=verified(&q,&answer);assert_eq!(facts.cancelled_prepare(),Some(123));assert!(facts.notified());
        assert!(d.sleeper.is_none());let sent=rt::SENDS.with(|s|s.get());
        assert_eq!(d.membership_close(&q,[4;32]).unwrap(),answer);assert_eq!(rt::SENDS.with(|s|s.get()),sent);
    }
    #[test] fn independent_pending_and_sleeper_are_preserved() {
        let mut d=Directory::default();let who=holder(2);let key=d.membership_grant(who,0,ALL,h()).unwrap();
        d.membership_grant(who,0,WRITE,g()).unwrap();
        d.pending=Some(PendingWrite{holder:who,key,request:321});d.sleeper=Some((who,89,4));
        let q=request();let answer=d.membership_close(&q,[4;32]).unwrap();
        assert_eq!(verified(&q,&answer).cancelled_prepare(),None);
        assert_eq!(d.pending.unwrap().request,321);assert!(d.sleeper.is_some());d.check(who,key,WRITE).unwrap();
    }
    #[test] fn ambiguous_lease_and_wrong_instance_cannot_ack() {
        let mut d=Directory::default();let who=holder(2);let key=d.grant(who,0,READ).unwrap();
        let q=request();assert_eq!(d.membership_close(&q,[5;32]),Err(rt::DENIED));
        assert_eq!(d.membership_close(&q,[4;32]),Err(rt::REFUSED));
        assert!(d.membership.is_none());d.check(who,key,READ).unwrap();
    }
    #[test] fn concurrent_different_withdrawal_cannot_replace_the_first() {
        let mut d=Directory::default();let who=holder(2);let key=d.membership_grant(who,0,ALL,h()).unwrap();
        let q=request();d.membership_close(&q,[4;32]).unwrap();
        let mut other=q;other[8]^=1;assert_eq!(d.membership_close(&other,[4;32]),Err(rt::WOULD_BLOCK));
        d.check(who,key,WRITE).unwrap();
        let mut other=q;other[104]=1;assert_eq!(d.membership_close(&other,[4;32]),Err(rt::BAD_ARGUMENT));
    }
    #[test] fn no_matching_group_is_not_a_session_revocation() {
        let mut d=Directory::default();let who=holder(2);let k=d.membership_grant(who,0,READ,direct()).unwrap();
        let q=request();let a=d.membership_close(&q,[4;32]).unwrap();let f=verified(&q,&a);
        assert_eq!((f.removed(),f.retained(),f.cancelled_prepare()),(0,1,None));d.check(who,k,READ).unwrap();
    }
    #[test] fn initial_group_membership_zero_is_revoked_without_touching_direct() {
        let mut d=Directory::default();let who=holder(2);
        let group=permission(Origin::Group([3;16]),WRITE,0);
        let g=d.membership_grant(who,0,WRITE,group).unwrap();
        let own=d.membership_grant(who,0,READ,direct()).unwrap();
        let mut q=request();q[56..64].fill(0);d.membership_close(&q,[4;32]).unwrap();
        assert_eq!(d.check(who,g,WRITE),Err(rt::REVOKED));d.check(who,own,READ).unwrap();
    }
    #[test] fn bounded_layout_and_full_table_refuses_without_losing_rights() {
        let mut d=Directory::default();let who=holder(2);
        for _ in 0..12 {d.membership_grant(who,0,READ,direct()).unwrap();}
        assert_eq!(d.membership_grant(who,0,READ,direct()),Err(rt::LIMIT));
        assert_eq!(d.leases.iter().flatten().count(),12);
        println!("Directory={} Lease={} Closure={} OptionClosure={}",size_of::<Directory>(),size_of::<Lease>(),size_of::<GroupClosure>(),size_of::<Option<GroupClosure>>());
        assert!(size_of::<Directory>()<=4096);
    }
    #[test] fn failed_wake_does_not_ack_or_remove_independent_new_leases() {
        let mut d=Directory::default();let who=holder(2);
        let key=d.membership_grant(who,0,WRITE,g()).unwrap();
        d.pending=Some(PendingWrite{holder:who,key,request:123});d.sleeper=Some((who,88,4));
        rt::STATUS.with(|s|s.set(rt::DENIED));let q=request();
        assert_eq!(d.membership_close(&q,[4;32]),Err(rt::DENIED));
        let own=d.membership_grant(who,0,READ,direct()).unwrap();
        assert_eq!(d.membership_grant(who,0,WRITE,g()),Err(rt::REVOKED));
        rt::STATUS.with(|s|s.set(rt::CLOSED));let a=d.membership_close(&q,[4;32]).unwrap();
        let facts=verified(&q,&a);assert!(facts.notified());
        assert_eq!(facts.cancelled_prepare(),Some(123));d.check(who,own,READ).unwrap();
        let sent=rt::SENDS.with(|s|s.get());
        assert_eq!(d.membership_close(&q,[4;32]).unwrap(),a);
        assert_eq!(rt::SENDS.with(|s|s.get()),sent);
        rt::STATUS.with(|s|s.set(rt::OK));
    }
    #[test] fn changed_sleeper_and_restarted_instance_do_not_synthesize_ack() {
        let mut d=Directory::default();let who=holder(2);
        let key=d.membership_grant(who,0,WRITE,g()).unwrap();
        d.pending=Some(PendingWrite{holder:who,key,request:123});d.sleeper=Some((who,88,4));
        rt::STATUS.with(|s|s.set(rt::WOULD_BLOCK));let q=request();
        assert_eq!(d.membership_close(&q,[4;32]),Err(rt::WOULD_BLOCK));
        d.sleeper=Some((holder(7),89,5));rt::STATUS.with(|s|s.set(rt::OK));
        let sent=rt::SENDS.with(|s|s.get());
        assert_eq!(d.membership_close(&q,[4;32]),Err(rt::REFUSED));
        assert_eq!(rt::SENDS.with(|s|s.get()),sent);
        assert!(d.sleeper.unwrap().0==holder(7));
        assert_eq!(Directory::default().membership_close(&q,[5;32]),Err(rt::DENIED));
    }

    #[test] fn later_retirement_requires_terminal_and_keeps_previous_barrier() {
        let mut d=Directory::default();let who=holder(2);
        let own=d.membership_grant(who,0,READ,direct()).unwrap();
        let gh=d.membership_grant(who,0,WRITE,g()).unwrap();let hh=d.membership_grant(who,0,ALL,h()).unwrap();
        let q=request();d.membership_close(&q,[4;32]).unwrap();
        let mut second=q;second[8..24].fill(2);second[40..56].fill(8);second[56..64].copy_from_slice(&6u64.to_le_bytes());second[64..72].copy_from_slice(&20u64.to_le_bytes());
        assert_eq!(d.membership_close(&second,[4;32]),Err(rt::WOULD_BLOCK));
        let mut end=q;end[..8].copy_from_slice(b"NXMGEND1");
        assert_eq!(d.membership_terminal(&end,[5;32]),Err(rt::DENIED));
        d.membership_terminal(&end,[4;32]).unwrap();d.membership_terminal(&end,[4;32]).unwrap();
        d.membership_close(&second,[4;32]).unwrap();
        assert_eq!(d.check(who,gh,WRITE),Err(rt::REVOKED));assert_eq!(d.check(who,hh,WRITE),Err(rt::REVOKED));d.check(who,own,READ).unwrap();
        assert_eq!(d.membership_grant(who,0,WRITE,g()),Err(rt::REVOKED));assert_eq!(d.membership_grant(who,0,WRITE,h()),Err(rt::REVOKED));
        let mut end=second;end[..8].copy_from_slice(b"NXMGEND1");d.membership_terminal(&end,[4;32]).unwrap();
        let mut third=second;third[8..24].fill(3);third[40..56].fill(9);
        assert_eq!(d.membership_close(&third,[4;32]),Err(rt::LIMIT));
        assert_eq!(d.membership_grant(who,0,WRITE,g()),Err(rt::REVOKED));assert_eq!(d.membership_grant(who,0,WRITE,h()),Err(rt::REVOKED));
    }

}
