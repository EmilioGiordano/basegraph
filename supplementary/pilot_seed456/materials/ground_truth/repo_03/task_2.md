# Feature: month-end statement

Month-end statements print several invoices at once and tell the account
which number comes next.

Add to `src/billing.rs`:

- `pub fn format_invoice_statement(invoices: &[Invoice]) -> String`: every invoice
  rendered with `format_invoice`, separated by a blank line, followed by a final line
  `NEXT #<n>` where `<n>` is the number the next issued invoice will get.
