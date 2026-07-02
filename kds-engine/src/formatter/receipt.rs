// Constantes ESC/POS (ESC = 0x1B, GS = 0x1D)
const INIT: &[u8] = &[0x1B, 0x40];
const BOLD_ON: &[u8] = &[0x1B, 0x45, 1];
const BOLD_OFF: &[u8] = &[0x1B, 0x45, 0];
const ALIGN_CENTER: &[u8] = &[0x1B, 0x61, 1];
const ALIGN_LEFT: &[u8] = &[0x1B, 0x61, 0];
const CUT_PARTIAL: &[u8] = &[0x1D, 0x56, 0x41, 5];
const LF: u8 = 0x0A;

pub struct TicketData<'a> {
    pub station_name: &'a str,
    pub order_number_short: &'a str,
    pub channel_icon: &'a str,
    pub customer_name: Option<&'a str>,
    pub lines: Vec<TicketLine<'a>>,
    pub time_hhmm: &'a str,
}

pub struct TicketLine<'a> {
    pub product_name: &'a str,
    pub quantity: i64,
    pub indent: usize,
    pub comment: Option<&'a str>,
}

/// Génère les bytes ESC/POS pour un ticket de préparation (80 mm, 42 colonnes).
#[must_use]
pub fn format_receipt(data: &TicketData<'_>) -> Vec<u8> {
    const WIDTH: usize = 42;
    let mut out = Vec::with_capacity(512);

    out.extend_from_slice(INIT);
    out.extend_from_slice(ALIGN_CENTER);
    out.extend_from_slice(BOLD_ON);

    let header = format!("{:<20}{:>22}", data.station_name, data.time_hhmm);
    out.extend_from_slice(header.as_bytes());
    out.push(LF);

    out.extend_from_slice(&[b'='; WIDTH]);
    out.push(LF);

    out.extend_from_slice(ALIGN_LEFT);
    let bon = format!("BON {}  {}", data.order_number_short, data.channel_icon);
    out.extend_from_slice(bon.as_bytes());
    out.push(LF);

    if let Some(name) = data.customer_name {
        out.extend_from_slice(name.as_bytes());
        out.push(LF);
    }

    out.extend_from_slice(BOLD_OFF);
    out.extend_from_slice(&[b'-'; WIDTH]);
    out.push(LF);

    let mut total_articles = 0i64;
    for line in &data.lines {
        let indent = "  ".repeat(line.indent);
        let qty_name = if line.indent == 0 {
            format!("{}{}x {}", indent, line.quantity, line.product_name)
        } else {
            format!("{}> {}", indent, line.product_name)
        };
        let truncated = if qty_name.len() > WIDTH {
            &qty_name[..WIDTH]
        } else {
            &qty_name
        };
        out.extend_from_slice(truncated.as_bytes());
        out.push(LF);

        if let Some(comment) = line.comment {
            let c = format!("  * {comment}");
            out.extend_from_slice(if c.len() > WIDTH { &c[..WIDTH] } else { &c }.as_bytes());
            out.push(LF);
        }

        if line.indent == 0 {
            total_articles += line.quantity;
        }
    }

    out.extend_from_slice(&[b'-'; WIDTH]);
    out.push(LF);

    let footer = format!(
        "{total_articles} article{}",
        if total_articles > 1 { "s" } else { "" }
    );
    out.extend_from_slice(footer.as_bytes());
    out.push(LF);
    out.push(LF);
    out.push(LF);
    out.extend_from_slice(CUT_PARTIAL);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_contains_station_and_order_number() {
        let data = TicketData {
            station_name: "GRILL",
            order_number_short: "A42",
            channel_icon: "CAISSE",
            customer_name: Some("Jean D."),
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
            ],
            time_hhmm: "14:32",
        };
        let bytes = format_receipt(&data);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("GRILL"));
        assert!(text.contains("A42"));
        assert!(text.contains("2x Burger Classic"));
        assert!(text.contains("sans oignon"));
        assert!(text.contains("> Pain brioche"));
        assert!(text.contains("2 articles"));
    }
}
