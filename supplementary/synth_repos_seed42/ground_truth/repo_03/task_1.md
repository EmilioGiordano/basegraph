# Feature: previews show the upcoming invoice number

Customers want to see, on a draft's preview, the number the invoice will
get when it is issued.

Add to `src/billing.rs`:

- `pub fn format_invoice_preview(invoice: &Invoice) -> String`: identical to `format_invoice` for
  issued invoices; for a draft the header line must be
  `INVOICE #<n> (preview)` where `<n>` is the number the invoice will
  receive when issued, followed by the usual lines and total.
