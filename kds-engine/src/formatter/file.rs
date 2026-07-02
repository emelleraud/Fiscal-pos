use std::fmt::Write as _;

use super::receipt::TicketData;

/// Génère un rendu texte WYSIWYG du ticket (mode test/mise au point).
/// Identique au rendu receipt mais en ASCII sans bytes de contrôle.
#[must_use]
pub fn format_file(data: &TicketData<'_>) -> String {
    let width = 42usize;
    let sep = "=".repeat(width);
    let dash = "-".repeat(width);
    let mut out = String::with_capacity(512);

    let _ = writeln!(out, "{:<20}{:>22}", data.station_name, data.time_hhmm);
    out.push_str(&sep);
    out.push('\n');
    let _ = writeln!(
        out,
        "BON {}  {}",
        data.order_number_short, data.channel_icon
    );

    if let Some(name) = data.customer_name {
        out.push_str(name);
        out.push('\n');
    }

    out.push_str(&dash);
    out.push('\n');

    let mut total_articles = 0i64;
    for line in &data.lines {
        let indent = "  ".repeat(line.indent);
        let text = if line.indent == 0 {
            format!("{}{}x {}", indent, line.quantity, line.product_name)
        } else {
            format!("{}> {}", indent, line.product_name)
        };
        let truncated = if text.len() > width {
            &text[..width]
        } else {
            &text
        };
        out.push_str(truncated);
        out.push('\n');

        if let Some(comment) = line.comment {
            let _ = writeln!(out, "  * {comment}");
        }
        if line.indent == 0 {
            total_articles += line.quantity;
        }
    }

    out.push_str(&dash);
    out.push('\n');
    let _ = writeln!(
        out,
        "{total_articles} article{}",
        if total_articles > 1 { "s" } else { "" }
    );

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::receipt::TicketLine;

    #[test]
    fn file_output_is_readable_ascii() {
        let data = TicketData {
            station_name: "FRITURE",
            order_number_short: "B03",
            channel_icon: "KIOSK",
            customer_name: None,
            lines: vec![TicketLine {
                product_name: "Frites L",
                quantity: 3,
                indent: 0,
                comment: None,
            }],
            time_hhmm: "12:00",
        };
        let text = format_file(&data);
        assert!(text.contains("FRITURE"));
        assert!(text.contains("B03"));
        assert!(text.contains("3x Frites L"));
        assert!(text.contains("3 articles"));
        // Pas de bytes ESC/POS
        assert!(!text.contains('\x1b'));
    }
}
