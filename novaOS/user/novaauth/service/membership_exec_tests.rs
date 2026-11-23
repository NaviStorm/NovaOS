// Harnais des structures d'inventaire ; le code testé est celui inclus dans exec.rs.
#[cfg(test)] mod tests {
    use super::*;
    fn ticket(scope:[u64;3],remote:bool)->Ticket {
        Ticket{issuer:1,binding:Binding{holder:[2;16],domain:[7;16],installation:[3;16]},scope,
               remote:remote.then_some(()),unchanged_state:123}
    }
    fn core(tickets:Vec<Ticket>)->Core {Core{processes:Processes{slots:vec![Some(Process{exec:tickets})]}}}
    #[test] fn independent_ready_or_admitted_tickets_are_preserved() {
        let s=core(vec![ticket([0;3],true)]);
        assert_eq!(membership_independent(&s,1,[2;16],[7;16]),Ok(1));
        assert_eq!(s.processes.slots[0].as_ref().unwrap().exec[0].unchanged_state,123);
    }
    #[test] fn unresolved_file_scope_or_missing_independent_admission_cannot_ack() {
        for t in [ticket([1,0,0],true),ticket([0,1,0],true),ticket([0,0,1],true),ticket([0;3],false)] {
            assert_eq!(membership_independent(&core(vec![t]),1,[2;16],[7;16]),Err(WOULD_BLOCK));
        }
    }
    #[test] fn different_subject_issuer_and_domain_are_not_touched() {
        let s=core(vec![ticket([99;3],true)]);
        assert_eq!(membership_independent(&s,9,[2;16],[7;16]),Ok(0));
        assert_eq!(membership_independent(&s,1,[9;16],[7;16]),Ok(0));
        assert_eq!(membership_independent(&s,1,[2;16],[9;16]),Ok(0));
    }
}
