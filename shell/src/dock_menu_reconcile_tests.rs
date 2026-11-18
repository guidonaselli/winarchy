use super::*;

#[test]
fn a_mutable_menu_borrow_defers_reconciliation_until_released() {
    MENU.with(|menu| {
        let guard = menu.borrow_mut();
        let during = can_reconcile();
        drop(guard);
        assert_eq!((during, can_reconcile()), (false, true));
    });
}

#[test]
fn a_shared_menu_borrow_defers_reconciliation_until_released() {
    MENU.with(|menu| {
        let guard = menu.borrow();
        let during = can_reconcile();
        drop(guard);
        assert_eq!((during, can_reconcile()), (false, true));
    });
}
