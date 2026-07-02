use super::receipt::{TicketData, TicketLine};

// Coupe partielle entre labels
const CUT_PARTIAL: &[u8] = &[0x1D, 0x56, 0x41, 5];
const INIT: &[u8] = &[0x1B, 0x40];
const LF: u8 = 0x0A;

/// Génère les bytes ESC/POS pour labels linerless (1 label par article racine).
/// `paper_width_mm` : 80 (42 colonnes) ou 50 (28 colonnes).
#[must_use]
pub fn format_linerless(data: &TicketData<'_>, paper_width_mm: i64) -> Vec<u8> {
    let cols = if paper_width_mm <= 50 {
        28usize
    } else {
        42usize
    };
    let mut out = Vec::with_capacity(512);
    out.extend_from_slice(INIT);

    // Un label par article racine (indent == 0)
    let mut i = 0;
    let lines = &data.lines;
    while i < lines.len() {
        if lines[i].indent != 0 {
            i += 1;
            continue;
        }

        // Collecter les enfants (composants/modifiers qui suivent)
        let mut label_lines: Vec<&TicketLine<'_>> = vec![&lines[i]];
        let mut j = i + 1;
        while j < lines.len() && lines[j].indent > 0 {
            label_lines.push(&lines[j]);
            j += 1;
        }

        let border = "=".repeat(cols);
        let inner_sep = "-".repeat(cols);

        // En-tête label
        let header = format!(
            "{} {} {}",
            data.order_number_short, data.station_name, data.time_hhmm
        );
        let header = if header.len() > cols {
            &header[..cols]
        } else {
            &header
        };
        out.extend_from_slice(border.as_bytes());
        out.push(LF);
        out.extend_from_slice(header.as_bytes());
        out.push(LF);
        out.extend_from_slice(inner_sep.as_bytes());
        out.push(LF);

        // Lignes de l'article
        for l in &label_lines {
            let text = if l.indent == 0 {
                format!("{}x {}", l.quantity, l.product_name)
            } else {
                format!("  > {}", l.product_name)
            };
            let text = if text.len() > cols {
                &text[..cols]
            } else {
                &text
            };
            out.extend_from_slice(text.as_bytes());
            out.push(LF);
            if let Some(c) = l.comment {
                let comment = format!("  * {c}");
                let comment = if comment.len() > cols {
                    &comment[..cols]
                } else {
                    &comment
                };
                out.extend_from_slice(comment.as_bytes());
                out.push(LF);
            }
        }

        out.extend_from_slice(border.as_bytes());
        out.push(LF);
        out.extend_from_slice(CUT_PARTIAL);

        i = j;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::receipt::TicketLine;

    #[test]
    fn linerless_80mm_two_items_two_labels() {
        let data = TicketData {
            station_name: "GRILL",
            order_number_short: "A42",
            channel_icon: "CAISSE",
            customer_name: None,
            lines: vec![
                TicketLine {
                    product_name: "Burger Classic",
                    quantity: 2,
                    indent: 0,
                    comment: Some("sans oignon"),
                },
                TicketLine {
                    product_name: "Pain brioche",
                    quantity: 2,
                    indent: 1,
                    comment: None,
                },
                TicketLine {
                    product_name: "Burger BBQ",
                    quantity: 1,
                    indent: 0,
                    comment: None,
                },
            ],
            time_hhmm: "14:32",
        };
        let bytes = format_linerless(&data, 80);
        let text = String::from_utf8_lossy(&bytes);
        // Deux coupes partielles = deux labels
        assert_eq!(
            bytes
                .windows(4)
                .filter(|w| *w == [0x1D, 0x56, 0x41, 5])
                .count(),
            2
        );
        assert!(text.contains("2x Burger Classic"));
        assert!(text.contains("1x Burger BBQ"));
    }
}
