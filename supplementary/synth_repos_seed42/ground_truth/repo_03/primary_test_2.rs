use tallyforge::billing::{format_invoice, format_invoice_statement, Invoice, Line, NEXT_INVOICE};
use std::sync::atomic::Ordering;

#[test]
fn statements_list_invoices_and_the_next_number() {
    let a = Invoice::draft(vec![Line::new("widget", 500)]).issue();
    let b = Invoice::draft(vec![Line::new("gadget", 700)]);
    let next = NEXT_INVOICE.load(Ordering::SeqCst);
    let text = format_invoice_statement(&[a.clone(), b]);
    assert!(text.contains(&format_invoice(&a)));
    assert!(text.contains("gadget 700\n"));
    assert!(text.contains("\n\n"), "{text:?}");
    assert!(text.ends_with(&format!("NEXT #{next}\n")), "{text:?}");
}
