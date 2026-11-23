#[cfg(test)] mod tests {
    use super::*;
    fn request()->Frame {let mut q=[0;128];q[..8].copy_from_slice(b"NXMGDAT1");q[8]=1;q[40]=7;q[72]=1;q[73]=3;q}
    #[test] fn wrong_lane_replay_and_saturation_preserve_original() {
        let mut x=Exchange::default();let mut seq=0;let q=request();
        assert_eq!(x.enqueue(1,&mut seq,1,q),Err(rt::DENIED));assert_eq!(seq,0);assert!(x.pending.is_none());
        assert_eq!(x.enqueue(0,&mut seq,1,q),Ok(()));
        assert_eq!(x.enqueue(0,&mut seq,1,q),Err(rt::DENIED));
        assert_eq!(x.enqueue(0,&mut seq,2,q),Err(rt::WOULD_BLOCK));
        assert_eq!(x.pending,Some((1,q)));assert_eq!(seq,1);
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
}
