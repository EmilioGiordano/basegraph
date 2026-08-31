use tallyforge::billing::{format_invoice, format_invoice_preview, Invoice, Line, NEXT_INVOICE};
use std::sync::atomic::Ordering;

#[test]
fn previews_show_the_upcoming_number() {
    let draft = Invoice::draft(vec![Line::new("widget", 500)]);
    let upcoming = NEXT_INVOICE.load(Ordering::SeqCst);
    let text = format_invoice_preview(&draft);
    assert!(
        text.starts_with(&format!("INVOICE #{upcoming} (preview)\n")),
        "{text:?}"
    );
    assert!(text.contains("widget 500\n"));
    assert!(text.ends_with("TOTAL 500\n"));
    let issued = draft.issue();
    assert_eq!(format_invoice_preview(&issued), format_invoice(&issued));
}
