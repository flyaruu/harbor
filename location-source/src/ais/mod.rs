mod types;

use std::error::Error;
use std::fmt;
use std::io::ErrorKind;
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

use http::Uri;
use serde::{Deserialize, Serialize};
use tungstenite::stream::MaybeTlsStream;
use tungstenite::Bytes;
use tungstenite::WebSocket;

use crate::module_bindings::upsert_ship_static_data_reducer::upsert_ship_static_data;
use crate::module_bindings::{add_location_report, DbConnection};

pub(crate) use types::*;

const AIS_READ_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) fn run_ais(conn: DbConnection) -> Result<(), Box<dyn Error>> {
    let mut reconnect_delay = Duration::from_secs(1);

    loop {
        match run_ais_session(&conn) {
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

fn run_ais_session(conn: &DbConnection) -> Result<(), Box<dyn Error>> {
    let aisstream_api_url: Uri = Uri::from_static("wss://stream.aisstream.io/v0/stream");
    let aisstream_api_key: String = std::env::var("AISSTREAM_API_KEY")
        .unwrap_or("".to_owned());

    println!("Connecting `{aisstream_api_url}`");
    let (mut socket, _) = tungstenite::connect(aisstream_api_url)?;
    set_socket_read_timeout(&mut socket, AIS_READ_TIMEOUT)?;

    let auth_request = AuthRequest {
        api_key: aisstream_api_key,
        bounding_boxes: vec![[[51.809685, 3.931732], [52.041860, 4.619751]]],
    };

    let auth_request_bytes = Bytes::copy_from_slice(&serde_json::to_vec(&auth_request)?);

    println!("Authenticating...");
    socket.send(tungstenite::Message::Binary(auth_request_bytes))?;

    println!("Successfully authenticated");
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
        process_message(conn, &message);
        message_count += 1;
        if message_count % 1000 == 0 {
            println!("Received {message_count} AIS messages");
        }
    }
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

fn process_message(conn: &DbConnection, message: &AisStreamMessage) {
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
