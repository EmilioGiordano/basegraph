use tallyforge::billing::{format_invoice_preview, Invoice, Line, NEXT_INVOICE};
use std::sync::atomic::Ordering;

#[test]
fn previewing_does_not_consume_numbers() {
    let draft = Invoice::draft(vec![Line::new("widget", 500)]);
    let before = NEXT_INVOICE.load(Ordering::SeqCst);
    let _ = format_invoice_preview(&draft);
    let _ = format_invoice_preview(&draft);
    assert_eq!(NEXT_INVOICE.load(Ordering::SeqCst), before);
}
