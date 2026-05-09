mod types;

use std::error::Error;
use std::fmt;

use http::Uri;
use serde::{Deserialize, Serialize};
use tungstenite::Bytes;

use crate::module_bindings::add_ship_reducer::add_ship;
use crate::module_bindings::{add_location_report, DbConnection};

pub(crate) use types::*;

pub(crate) fn run_ais(conn: DbConnection) -> Result<(), Box<dyn Error>> {
    let aisstream_api_url: Uri = Uri::from_static("wss://stream.aisstream.io/v0/stream");
    let aisstream_api_key: String = std::env::var("AISSTREAM_API_KEY")
        .unwrap_or("401620aea8c9f66129af3d8b1caec95a86144a61".to_owned());

    println!("Connecting `{aisstream_api_url}`");
    let (mut socket, _) = tungstenite::connect(aisstream_api_url)?;

    let auth_request = AuthRequest {
        api_key: aisstream_api_key,
        bounding_boxes: vec![[[51.809685, 3.931732], [52.041860, 4.619751]]],
    };

    let auth_request_bytes = Bytes::copy_from_slice(&serde_json::to_vec(&auth_request)?);

    println!("Authenticating...");
    socket.send(tungstenite::Message::Binary(auth_request_bytes))?;
    // let message = match socket.read()? {
    //     tungstenite::Message::Binary(message) => {
    //         println!("Received authentication response: {:?}", message);
    //         match serde_json::from_slice::<AuthMessage>(&message)? {
    //             AuthMessage::AuthError(message) => {
    //                 return Err(format!("Authentication error: {message:?}").into());
    //             }
    //             AuthMessage::Message(message) => Some(message),
    //             _ => None,
    //         }
    //     }
    //     _ => None,
    // };

    println!("Successfully authenticated");
    // if let Some(message) = message {
    //     print_message(&message);
    // }

    loop {
        println!("Waiting for messages...");
        let message = match socket.read()? {
            tungstenite::Message::Binary(message) => {
                serde_json::from_slice::<AisStreamMessage>(&message)?
            }
            tungstenite::Message::Close(message) => {
                return Err(format!("Connection closed: {message:?}").into());
            }
            _ => continue,
        };
        print_message(&conn, &message);
    }
}

fn print_message(conn: &DbConnection, message: &AisStreamMessage) {
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
            println!(
                ">>>>>>>{} position: lat={}, lon={}, sog={}, cog={}",
                metadata.ship_name, body.latitude, body.longitude, body.sog, body.cog
            );
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
            println!(
                ">>>>>>>{} class B position: lat={}, lon={}, sog={}, cog={}",
                metadata.ship_name, body.latitude, body.longitude, body.sog, body.cog
            );
        }
        AisStreamMessage::ShipStaticData { body, .. } => {
            conn.reducers
                .add_ship(body.name.clone(), Some(body.call_sign.clone()))
                .unwrap();
            println!(
                ">>>>>>>{} static data: callsign={}, destination={}",
                body.name, body.call_sign, body.destination
            );
        }
        AisStreamMessage::UnknownMessage { metadata, .. } => {
            println!("{} unknown AIS message", metadata.ship_name);
        }
        AisStreamMessage::Other { .. } => {}
    }

    let pretty_message = serde_json::to_string_pretty(message)
        .unwrap_or_else(|_| "Failed to serialize message".to_string());
    println!("{pretty_message}");
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
