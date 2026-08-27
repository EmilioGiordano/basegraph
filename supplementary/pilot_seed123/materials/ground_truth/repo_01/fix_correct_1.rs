//! Invoices: numbering sequence, drafts and rendering.

use std::sync::atomic::{AtomicU64, Ordering};

/// Global invoice sequence; every issued invoice takes the next number.
pub static NEXT_INVOICE: AtomicU64 = AtomicU64::new(1000);

pub fn issue_number() -> u64 {
    NEXT_INVOICE.fetch_add(1, Ordering::SeqCst)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Line {
    pub item: String,
    pub cents: u64,
}

impl Line {
    pub fn new(item: &str, cents: u64) -> Self {
        Self {
            item: item.to_string(),
            cents,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Invoice {
    pub number: u64,
    pub lines: Vec<Line>,
}

impl Invoice {
    /// An unissued invoice (number 0).
    pub fn draft(lines: Vec<Line>) -> Self {
        Self { number: 0, lines }
    }

    pub fn issue(mut self) -> Self {
        self.number = issue_number();
        self
    }

    pub fn total(&self) -> u64 {
        self.lines.iter().map(|l| l.cents).sum()
    }
}

/// Render an invoice for printing or preview.
pub fn render_invoice(invoice: &Invoice) -> String {
    let header = match invoice.number {
        0 => "INVOICE DRAFT".to_string(),
        n => format!("INVOICE #{n}"),
    };
    let body: Vec<String> = invoice
        .lines
        .iter()
        .map(|line| format!("{} {}", line.item, line.cents))
        .collect();
    let mut out = header;
    out.push('\n');
    for line in body {
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&format!("TOTAL {}\n", invoice.total()));
    out
}

/// Preview of an invoice; drafts show the number they will receive.
pub fn render_invoice_preview(invoice: &Invoice) -> String {
    if invoice.number != 0 {
        return render_invoice(invoice);
    }
    let upcoming = NEXT_INVOICE.load(Ordering::SeqCst);
    let body = render_invoice(invoice);
    let body = body.strip_prefix("INVOICE DRAFT\n").unwrap_or(&body);
    format!("INVOICE #{upcoming} (preview)\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issued_invoices_show_their_number() {
        let invoice = Invoice::draft(vec![Line::new("widget", 500)]).issue();
        let text = render_invoice(&invoice);
        assert!(text.starts_with(&format!("INVOICE #{}\n", invoice.number)));
        assert!(text.contains("widget 500\n"));
        assert!(text.ends_with("TOTAL 500\n"));
    }

    #[test]
    fn previewing_a_draft_is_stable() {
        let draft = Invoice::draft(vec![Line::new("widget", 500)]);
        assert_eq!(render_invoice(&draft), render_invoice(&draft));
        assert!(render_invoice(&draft).starts_with("INVOICE DRAFT\n"));
    }
}
