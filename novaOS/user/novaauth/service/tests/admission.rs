use novaos_auth_session::admission::Admission;

#[test]
fn reprise_origine_principal_et_budget_global() {
    let a = [1; 16];
    let b = [2; 16];
    let mut budget = Admission::new(1000);
    assert!(!budget.allowed(0, a, 1639));
    assert!(budget.allowed(0, a, 1640));
    budget.fail(0, a, 1640);
    assert!(!budget.allowed(1, b, 1649));
    assert!(budget.allowed(1, b, 1650));
    assert!(!budget.allowed(0, b, 1651));
    assert!(!budget.allowed(1, a, 1651));
    assert!(budget.allowed(1, a, 1660));
    budget.fail(1, a, 1660);
    assert!(!budget.allowed(0, a, 1699));
    assert!(budget.allowed(0, a, 1700));
    budget.success(0, a);
    assert!(!budget.allowed(2, a, 2000));
}
