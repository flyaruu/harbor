mod types;

use std::error::Error;
use std::fmt;
use std::fs::{self, File};
use std::io::ErrorKind;
use std::io::{LineWriter, Write};
use std::net::TcpStream;
use std::path::Path;
use std::str::from_utf8;
use std::thread;
use std::time::Duration;

use http::Uri;
use native_tls::TlsConnector;
use serde::{Deserialize, Serialize};
use spacetimedb_sdk::Timestamp;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Bytes;
use tungstenite::WebSocket;
use tungstenite::{client_tls_with_config, Connector};

use crate::module_bindings::upsert_ship_static_data_reducer::upsert_ship_static_data;
use crate::module_bindings::{add_location_report, DbConnection};

pub(crate) use types::*;

const AIS_READ_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize)]
struct LocationReportLogEntry {
    received_at: String,
    source: &'static str,
    mmsi: u64,
    ship_name: String,
    lat: f64,
    lon: f64,
    cog: f64,
    sog: f64,
}

#[derive(Serialize)]
struct ShipStaticDataLogEntry {
    received_at: String,
    source: &'static str,
    mmsi: u64,
    name: String,
    call_sign: String,
    destination: String,
    dimension_a: u16,
    dimension_b: u16,
    dimension_c: u16,
    dimension_d: u16,
    dte: bool,
    eta_month: u8,
    eta_day: u8,
    eta_hour: u8,
    eta_minute: u8,
    fix_type: u8,
    imo_number: u32,
    maximum_static_draught: f64,
    ship_type: u8,
    ais_version: u8,
}

struct AisFileLogger {
    location_reports: LineWriter<File>,
    ship_static_data: LineWriter<File>,
}

impl AisFileLogger {
    fn new() -> Result<Self, Box<dyn Error>> {
        let output_dir = Path::new("output");
        fs::create_dir_all(output_dir)?;

        let run_number = next_run_number(output_dir)?;
        let location_reports_path =
            output_dir.join(format!("run_{run_number:04}_location_reports.jsonl"));
        let ship_static_data_path =
            output_dir.join(format!("run_{run_number:04}_ship_static_data.jsonl"));

        Ok(Self {
            location_reports: LineWriter::new(File::create(location_reports_path)?),
            ship_static_data: LineWriter::new(File::create(ship_static_data_path)?),
        })
    }

    fn log_location_report(
        &mut self,
        entry: &LocationReportLogEntry,
    ) -> Result<(), Box<dyn Error>> {
        serde_json::to_writer(&mut self.location_reports, entry)?;
        self.location_reports.write_all(b"\n")?;
        Ok(())
    }

    fn log_ship_static_data(
        &mut self,
        entry: &ShipStaticDataLogEntry,
    ) -> Result<(), Box<dyn Error>> {
        serde_json::to_writer(&mut self.ship_static_data, entry)?;
        self.ship_static_data.write_all(b"\n")?;
        Ok(())
    }
}

pub(crate) fn run_ais(conn: DbConnection) -> Result<(), Box<dyn Error>> {
    let mut reconnect_delay = Duration::from_secs(1);
    let mut logger = AisFileLogger::new()?;

    loop {
        match run_ais_session(&conn, &mut logger) {
            Ok(()) => {
                reconnect_delay = Duration::from_secs(1);
            }
            Err(err) => {
                eprintln!(
                    "AIS stream disconnected: {err}. Reconnecting in {}s...",
                    reconnect_delay.as_secs()
                );
                thread::sleep(reconnect_delay);
                reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(30));
            }
        }
    }
}

fn run_ais_session(conn: &DbConnection, logger: &mut AisFileLogger) -> Result<(), Box<dyn Error>> {
    let aisstream_api_url: Uri = Uri::from_static("wss://stream.aisstream.io/v0/stream");
    let aisstream_api_key: String = std::env::var("AISSTREAM_API_KEY").unwrap_or("".to_owned());
    let no_cert_validation = no_cert_validation_enabled();

    println!("Connecting `{aisstream_api_url}`");
    let (mut socket, _) = connect_ais_socket(&aisstream_api_url, no_cert_validation)?;
    set_socket_read_timeout(&mut socket, AIS_READ_TIMEOUT)?;

    let auth_request = AuthRequest {
        api_key: aisstream_api_key,
        bounding_boxes: vec![[[51.809685, 3.931732], [52.041860, 4.619751]]],
    };

    let auth_request_bytes = Bytes::copy_from_slice(&serde_json::to_vec(&auth_request)?);

    println!("Authenticating...");
    let auth_message = serde_json::to_string(&auth_request)?;
    socket.send(tungstenite::Message::Binary(auth_request_bytes))?;

    println!("Successfully authenticated: {}",auth_message);
    let mut message_count = 0;
    loop {
        let message = match socket.read() {
            Err(tungstenite::Error::Io(err))
                if matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock) =>
            {
                return Err(format!(
                    "No AIS messages received for {} seconds",
                    AIS_READ_TIMEOUT.as_secs()
                )
                .into());
            }
            Err(err) => return Err(err.into()),
            Ok(tungstenite::Message::Binary(message)) => {
                serde_json::from_slice::<AisStreamMessage>(&message)?
            }
            Ok(tungstenite::Message::Close(message)) => {
                return Err(format!("Connection closed: {message:?}").into());
            }
            Ok(_) => continue,
        };
        process_message(conn, logger, &message);
        message_count += 1;
        if message_count % 1000 == 0 {
            println!("Received {message_count} AIS messages");
        }
    }
}

fn connect_ais_socket(
    uri: &Uri,
    no_cert_validation: bool,
) -> Result<
    (
        WebSocket<MaybeTlsStream<TcpStream>>,
        http::Response<Option<Vec<u8>>>,
    ),
    Box<dyn Error>,
> {
    let host = uri.host().ok_or("AIS stream URI missing host")?;
    let port = uri.port_u16().unwrap_or(443);
    let stream = TcpStream::connect((host, port))?;
    stream.set_nodelay(true)?;

    let connector = if no_cert_validation {
        eprintln!(
            "NO_CERT_VALIDATION=true, TLS certificate and hostname validation disabled for AIS stream"
        );
        let mut builder = TlsConnector::builder();
        builder.danger_accept_invalid_certs(true);
        builder.danger_accept_invalid_hostnames(true);
        Some(Connector::NativeTls(builder.build()?))
    } else {
        None
    };

    let (socket, response) = client_tls_with_config(uri.clone(), stream, None, connector).map_err(
        |error| match error {
            tungstenite::HandshakeError::Failure(error) => error,
            tungstenite::HandshakeError::Interrupted(_) => {
                tungstenite::Error::Io(std::io::Error::other("AIS TLS handshake interrupted"))
            }
        },
    )?;

    Ok((socket, response))
}

fn no_cert_validation_enabled() -> bool {
    std::env::var("NO_CERT_VALIDATION")
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or(false)
}

fn next_run_number(output_dir: &Path) -> Result<u32, Box<dyn Error>> {
    let mut max_run = 0;

    for entry in fs::read_dir(output_dir)? {
        let entry = entry?;
        let Some(file_name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };

        let Some(rest) = file_name.strip_prefix("run_") else {
            continue;
        };
        let Some((run_number, _)) = rest.split_once('_') else {
            continue;
        };
        let Ok(run_number) = run_number.parse::<u32>() else {
            continue;
        };
        max_run = max_run.max(run_number);
    }

    Ok(max_run + 1)
}

fn received_at_string() -> String {
    Timestamp::now()
        .to_rfc3339()
        .unwrap_or_else(|_| Timestamp::now().to_micros_since_unix_epoch().to_string())
}

fn clean_string(value: &str) -> String {
    value.replace(['\r', '\n'], " ").trim().to_string()
}

fn set_socket_read_timeout(
    socket: &mut WebSocket<MaybeTlsStream<TcpStream>>,
    timeout: Duration,
) -> Result<(), Box<dyn Error>> {
    match socket.get_mut() {
        MaybeTlsStream::Plain(stream) => stream.set_read_timeout(Some(timeout))?,
        MaybeTlsStream::NativeTls(stream) => stream.get_mut().set_read_timeout(Some(timeout))?,
        #[allow(unreachable_patterns)]
        _ => {}
    }

    Ok(())
}

fn process_message(conn: &DbConnection, logger: &mut AisFileLogger, message: &AisStreamMessage) {
    match message {
        AisStreamMessage::PositionReport { metadata, body } => {
            conn.reducers
                .add_location_report(
                    metadata.mmsi,
                    body.latitude,
                    body.longitude,
                    Some(body.cog),
                    Some(body.sog),
                )
                .unwrap();

            logger
                .log_location_report(&LocationReportLogEntry {
                    received_at: received_at_string(),
                    source: "PositionReport",
                    mmsi: metadata.mmsi,
                    ship_name: clean_string(&metadata.ship_name),
                    lat: body.latitude,
                    lon: body.longitude,
                    cog: body.cog,
                    sog: body.sog,
                })
                .unwrap();
        }
        AisStreamMessage::StandardClassBPositionReport { metadata, body } => {
            conn.reducers
                .add_location_report(
                    metadata.mmsi,
                    body.latitude,
                    body.longitude,
                    Some(body.cog),
                    Some(body.sog),
                )
                .unwrap();

            logger
                .log_location_report(&LocationReportLogEntry {
                    received_at: received_at_string(),
                    source: "StandardClassBPositionReport",
                    mmsi: metadata.mmsi,
                    ship_name: clean_string(&metadata.ship_name),
                    lat: body.latitude,
                    lon: body.longitude,
                    cog: body.cog,
                    sog: body.sog,
                })
                .unwrap();
        }
        AisStreamMessage::ShipStaticData { metadata, body } => {
            conn.reducers
                .upsert_ship_static_data(
                    metadata.mmsi,
                    body.name.clone(),
                    body.call_sign.clone(),
                    body.destination.clone(),
                    body.dimension.a,
                    body.dimension.b,
                    body.dimension.c,
                    body.dimension.d,
                    body.dte,
                    body.eta.month,
                    body.eta.day,
                    body.eta.hour,
                    body.eta.minute,
                    body.fix_type,
                    body.imo_number,
                    body.maximum_static_draught,
                    body.ship_type,
                    body.ais_version,
                )
                .unwrap();

            logger
                .log_ship_static_data(&ShipStaticDataLogEntry {
                    received_at: received_at_string(),
                    source: "ShipStaticData",
                    mmsi: metadata.mmsi,
                    name: clean_string(&body.name),
                    call_sign: clean_string(&body.call_sign),
                    destination: clean_string(&body.destination),
                    dimension_a: body.dimension.a,
                    dimension_b: body.dimension.b,
                    dimension_c: body.dimension.c,
                    dimension_d: body.dimension.d,
                    dte: body.dte,
                    eta_month: body.eta.month,
                    eta_day: body.eta.day,
                    eta_hour: body.eta.hour,
                    eta_minute: body.eta.minute,
                    fix_type: body.fix_type,
                    imo_number: body.imo_number,
                    maximum_static_draught: body.maximum_static_draught,
                    ship_type: body.ship_type,
                    ais_version: body.ais_version,
                })
                .unwrap();
        }
        AisStreamMessage::UnknownMessage { metadata, .. } => {
            println!("{} unknown AIS message", metadata.ship_name);
        }
        AisStreamMessage::Other { .. } => {}
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthRequest {
    #[serde(rename = "APIKey")]
    pub api_key: String,
    #[serde(rename = "BoundingBoxes")]
    pub bounding_boxes: Vec<[[f64; 2]; 2]>,
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(untagged)]
pub enum AuthMessage {
    AuthError(AuthError),
    Message(AisStreamMessage),
}

#[allow(dead_code)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AuthError {
    pub error: String,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.error.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unknown_message() {
        let message: AisStreamMessage =
            serde_json::from_str(include_str!("../../tests/data/aisstream/message1.json")).unwrap();

        assert!(matches!(message, AisStreamMessage::UnknownMessage { .. }));
        assert_eq!(message.metadata().ship_name, "POROS");
    }

    #[test]
    fn parses_ship_static_data() {
        let message: AisStreamMessage =
            serde_json::from_str(include_str!("../../tests/data/aisstream/message2.json")).unwrap();

        let AisStreamMessage::ShipStaticData { body, .. } = message else {
            panic!("expected ship static data");
        };
        assert_eq!(body.name, "SINCFAL");
        assert_eq!(body.user_id, 244740748);
    }

    #[test]
    fn parses_standard_class_b_position_report() {
        let message: AisStreamMessage =
            serde_json::from_str(include_str!("../../tests/data/aisstream/message3.json")).unwrap();

        let AisStreamMessage::StandardClassBPositionReport { body, .. } = message else {
            panic!("expected standard class b position report");
        };
        assert_eq!(body.user_id, 244620651);
        assert!(body.position_accuracy);
    }

    #[test]
    fn parses_position_report() {
        let message: AisStreamMessage =
            serde_json::from_str(include_str!("../../tests/data/aisstream/message4.json")).unwrap();

        let AisStreamMessage::PositionReport { body, .. } = message else {
            panic!("expected position report");
        };
        assert_eq!(body.user_id, 245444000);
        assert_eq!(body.longitude, 4.148315);
    }
}
