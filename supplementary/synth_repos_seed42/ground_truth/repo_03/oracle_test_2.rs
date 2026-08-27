use tallyforge::billing::{format_invoice_statement, Invoice, Line, NEXT_INVOICE};
use std::sync::atomic::Ordering;

#[test]
fn printing_a_statement_does_not_consume_numbers() {
    let invoices = vec![
        Invoice::draft(vec![Line::new("widget", 500)]),
        Invoice::draft(vec![Line::new("gadget", 700)]),
    ];
    let before = NEXT_INVOICE.load(Ordering::SeqCst);
    let _ = format_invoice_statement(&invoices);
    assert_eq!(NEXT_INVOICE.load(Ordering::SeqCst), before);
}
