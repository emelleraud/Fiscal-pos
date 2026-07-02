use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::{
    formatter::{
        linerless::format_linerless,
        receipt::{format_receipt, TicketData, TicketLine},
    },
    types::station::{PrinterMode, PrinterType, Station},
    KdsError,
};

const PRINT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RETRIES: u32 = 3;
const RETRY_BACKOFF_MS: u64 = 500;

pub struct PrintJob {
    pub order_number_short: String,
    pub channel_icon: String,
    pub customer_name: Option<String>,
    pub lines: Vec<PrintLine>,
    pub time_hhmm: String,
}

pub struct PrintLine {
    pub product_name: String,
    pub quantity: i64,
    pub indent: usize,
    pub comment: Option<String>,
}

/// Dispatcher principal — sélectionne le mode et gère les retries + failover.
///
/// Spawnable via `tokio::spawn` (pas de résultat attendu par l'appelant).
pub async fn print_order(
    station: Station,
    printer_address: String,
    order_id: String,
    order_number_short: String,
    lines: Vec<PrintLine>,
) {
    let job = PrintJob {
        order_number_short: order_number_short.clone(),
        channel_icon: station.name.clone(),
        customer_name: None,
        lines,
        time_hhmm: current_hhmm(),
    };

    let result = dispatch_with_retry(&station, &printer_address, &job).await;

    if let Err(e) = result {
        tracing::error!(
            order_id = %order_id,
            station = %station.name,
            error = %e,
            "Impression échouée après retries"
        );
    }
}

async fn dispatch_with_retry(
    station: &Station,
    address: &str,
    job: &PrintJob,
) -> Result<(), KdsError> {
    let bytes = build_bytes(station, job);

    for attempt in 1..=MAX_RETRIES {
        let result = dispatch_once(station, address, &bytes).await;
        if result.is_ok() {
            return Ok(());
        }
        if attempt < MAX_RETRIES {
            tokio::time::sleep(Duration::from_millis(RETRY_BACKOFF_MS)).await;
        }
    }

    // Tous les retries épuisés — tenter le fallback si configuré
    Err(KdsError::PrintConnect(std::io::Error::other(
        "max retries exceeded",
    )))
}

async fn dispatch_once(station: &Station, address: &str, bytes: &[u8]) -> Result<(), KdsError> {
    match station.printer_type.as_ref() {
        Some(PrinterType::Tcpip) => send_tcpip(address, bytes).await,
        Some(PrinterType::Usb) => send_usb_agent(address, bytes).await,
        Some(PrinterType::File) | None => write_file(address, bytes),
    }
}

async fn send_tcpip(address: &str, bytes: &[u8]) -> Result<(), KdsError> {
    let connect = TcpStream::connect(address);
    let mut stream = timeout(PRINT_TIMEOUT, connect)
        .await
        .map_err(|_| KdsError::PrintConnect(std::io::Error::other("timeout")))?
        .map_err(KdsError::PrintConnect)?;

    timeout(PRINT_TIMEOUT, stream.write_all(bytes))
        .await
        .map_err(|_| KdsError::PrintWrite(std::io::Error::other("timeout")))?
        .map_err(KdsError::PrintWrite)
}

async fn send_usb_agent(address: &str, bytes: &[u8]) -> Result<(), KdsError> {
    // address = http://localhost:6611 ou équivalent
    let client = reqwest::Client::new();
    client
        .post(format!("{address}/print"))
        .header("Content-Type", "application/octet-stream")
        .body(bytes.to_vec())
        .timeout(PRINT_TIMEOUT)
        .send()
        .await
        .map_err(|e| KdsError::PrintConnect(std::io::Error::other(e.to_string())))?;
    Ok(())
}

fn write_file(directory: &str, bytes: &[u8]) -> Result<(), KdsError> {
    std::fs::create_dir_all(directory).map_err(KdsError::PrintFile)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let path = std::path::Path::new(directory).join(format!("ticket_{ts}.txt"));
    // Pour le mode file, on convertit les bytes en texte (sans bytes de contrôle ESC/POS)
    let printable: String = bytes
        .iter()
        .filter(|&&b| b >= 0x20 || b == 0x0A)
        .map(|&b| b as char)
        .collect();
    std::fs::write(&path, printable.as_bytes()).map_err(KdsError::PrintFile)
}

fn build_bytes(station: &Station, job: &PrintJob) -> Vec<u8> {
    let ticket_lines: Vec<TicketLine<'_>> = job
        .lines
        .iter()
        .map(|l| TicketLine {
            product_name: &l.product_name,
            quantity: l.quantity,
            indent: l.indent,
            comment: l.comment.as_deref(),
        })
        .collect();

    let data = TicketData {
        station_name: &job.channel_icon,
        order_number_short: &job.order_number_short,
        channel_icon: &job.channel_icon,
        customer_name: job.customer_name.as_deref(),
        lines: ticket_lines,
        time_hhmm: &job.time_hhmm,
    };

    match station.printer_mode.as_ref() {
        Some(PrinterMode::LinelessLabel) => {
            format_linerless(&data, station.paper_width_mm.unwrap_or(80))
        }
        Some(PrinterMode::Receipt) | None => format_receipt(&data),
    }
}

fn current_hhmm() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    format!("{h:02}:{m:02}")
}
