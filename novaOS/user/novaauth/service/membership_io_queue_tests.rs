#[cfg(test)] mod tests {
    use super::*;
    fn request()->Frame {let mut q=[0;128];q[..8].copy_from_slice(b"NXMGDAT1");q[8]=1;q[40]=7;q[72]=1;q[73]=3;q}
    #[test] fn wrong_lane_replay_and_saturation_preserve_original() {
        let mut x=Exchange::default();let mut seq=0;let q=request();
        assert_eq!(x.enqueue(1,&mut seq,1,q),Err(rt::DENIED));assert_eq!(seq,0);assert!(x.pending.is_none());
        assert_eq!(x.enqueue(0,&mut seq,1,q),Ok(()));
        assert_eq!(x.enqueue(0,&mut seq,1,q),Err(rt::DENIED));
        assert_eq!(x.enqueue(0,&mut seq,2,q),Err(rt::WOULD_BLOCK));
        assert_eq!(x.pending,Some((1,q[..80].try_into().unwrap())));assert_eq!(seq,1);
        x.pending=None;assert_eq!(x.enqueue(0,&mut seq,2,q),Ok(()));
    }
    #[test] fn malformed_requests_never_allocate_or_advance_sequence() {
        for at in [0,8,40,72,73,74,127] {
            let mut x=Exchange::default();let mut seq=0;let mut q=request();
            q[at]=if matches!(at,74|127){1}else{0};
            assert_eq!(x.enqueue(0,&mut seq,1,q),Err(rt::BAD_ARGUMENT),"offset {at}");
            assert_eq!(seq,0);assert!(x.pending.is_none());assert!(x.context.is_none());
        }
        let mut x=Exchange::default();let mut seq=0;let mut q=request();q[72]=2;
        assert_eq!(x.enqueue(0,&mut seq,1,q),Err(rt::BAD_ARGUMENT));
        q[64]=5;assert_eq!(x.enqueue(0,&mut seq,1,q),Ok(()));
    }

    #[test] fn lost_delivery_keeps_exact_fact_and_output_and_blocks_new_mutation() {
        let mut x=Exchange::default();let mut fact=[0;128];fact[8..16].copy_from_slice(b"NXMGIOA1");fact[16..48].fill(7);
        let mut output=[0;128];output[..8].copy_from_slice(b"NXMGOUT1");output[32]=42;
        x.context=Some(fact[..120].try_into().unwrap());x.pending=Some((19,output[..80].try_into().unwrap()));
        rt::STATUS.with(|v|v.set(rt::WOULD_BLOCK));
        assert_eq!(x.deliver(),Err(22));assert_eq!(x.pending,Some((19,output[..80].try_into().unwrap())));
        assert_eq!(x.remembered(&[7;32]),Ok(fact));assert_eq!(x.remembered(&[8;32]),Err(rt::DENIED));
        assert_eq!(x.enqueue(0,&mut 19,20,request()),Err(rt::WOULD_BLOCK));
        rt::STATUS.with(|v|v.set(0));assert_eq!(x.deliver(),Ok(()));assert!(x.pending.is_none());
        let calls=rt::SENDS.with(|v|v.get());x.deliver().unwrap();assert_eq!(rt::SENDS.with(|v|v.get()),calls);
        assert_eq!(x.remembered(&[7;32]),Ok(fact));
        assert_eq!(x.enqueue(0,&mut 19,20,request()),Err(rt::WOULD_BLOCK));
    }
    #[test] fn stale_context_is_never_a_completed_operation() {
        let x=Exchange{context:Some(request()[..120].try_into().unwrap()),pending:None};
        assert_eq!(x.remembered(&[0;32]),Err(rt::DENIED));
    }

    #[test] fn compact_frames_preserve_every_useful_byte_and_refuse_truncation() {
        assert_eq!(compact::<129>(&[0;128]),Err(rt::BAD_ARGUMENT));
        for width in [80usize,120] {
            let mut full=[0;128];for (i,b) in full[..width].iter_mut().enumerate(){*b=(i+1) as u8;}
            let mut restored=[0;128];
            if width==80 {restored[..80].copy_from_slice(&compact::<80>(&full).unwrap());}
            else {restored[..120].copy_from_slice(&compact::<120>(&full).unwrap());}
            assert_eq!(restored,full);
            for at in width..128 {let mut invalid=full;invalid[at]=1;
                if width==80{assert_eq!(compact::<80>(&invalid),Err(rt::BAD_ARGUMENT));}
                else{assert_eq!(compact::<120>(&invalid),Err(rt::BAD_ARGUMENT));}
            }
        }
    }

    fn cancellation(q:&Frame,tag:u64)->Frame {
        let mut c=[0;128];c[..8].copy_from_slice(b"NXMGCAN1");c[8..16].copy_from_slice(&tag.to_le_bytes());
        c[16..48].copy_from_slice(&q[8..40]);c[48..80].copy_from_slice(&novaos_identity::recuperation_view::seal_digest(q));c
    }
    #[test] fn queued_cancellation_has_exact_binding_and_no_data_or_new_allocation() {
        let mut x=Exchange::default();let q=request();x.enqueue(0,&mut 0,11,q).unwrap();
        let c=cancellation(&q,11);let original=x.pending;
        for i in [8,16,48,79,127] {
            let mut wrong=c;wrong[i]^=1;assert!(x.cancel_queued(&wrong,[4;32]).is_err());
            assert_eq!(x.pending,original);assert!(x.context.is_none());
        }
        let f=x.cancel_queued(&c,[4;32]).unwrap();
        assert_eq!(f[48..80],[4;32]);assert_eq!(f[112..120],rt::CANCELLED.to_le_bytes());
        let output=x.pending.unwrap().1;assert_eq!(output[8..16],rt::CANCELLED.to_le_bytes());assert_eq!(output[16..],[0;64]);
        assert_eq!(x.cancel_queued(&c,[4;32]),Err(rt::WOULD_BLOCK));
        rt::STATUS.with(|v|v.set(rt::WOULD_BLOCK));assert_eq!(x.deliver(),Err(22));
        assert_eq!(x.remembered(&novaos_identity::recuperation_view::seal_digest(&c)),Ok(f));
        rt::STATUS.with(|v|v.set(0));x.deliver().unwrap();assert!(x.pending.is_none());
        assert_eq!(x.remembered(&novaos_identity::recuperation_view::seal_digest(&c)),Ok(f));
        assert_eq!(core::mem::size_of::<Exchange>(),224);
    }
    #[test] fn a_go_context_prevents_queued_cancellation() {
        let mut x=Exchange::default();let q=request();x.enqueue(0,&mut 0,11,q).unwrap();
        x.context=Some([9;120]);let saved=x.pending;
        assert_eq!(x.cancel_queued(&cancellation(&q,11),[4;32]),Err(rt::WOULD_BLOCK));assert_eq!(x.pending,saved);
    }
}
