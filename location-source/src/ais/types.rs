use std::collections::BTreeMap;

use serde::{ser::SerializeStruct, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AisMetadata {
    #[serde(rename = "MMSI")]
    pub mmsi: u64,
    #[serde(rename = "MMSI_String")]
    pub mmsi_string: u64,
    #[serde(rename = "ShipName")]
    pub ship_name: String,
    pub latitude: f64,
    pub longitude: f64,
    pub time_utc: String,
}

#[derive(Clone, Debug)]
pub enum AisStreamMessage {
    UnknownMessage {
        metadata: AisMetadata,
        body: UnknownMessage,
    },
    ShipStaticData {
        metadata: AisMetadata,
        body: ShipStaticData,
    },
    StandardClassBPositionReport {
        metadata: AisMetadata,
        body: StandardClassBPositionReport,
    },
    PositionReport {
        metadata: AisMetadata,
        body: PositionReport,
    },
    Other {
        metadata: AisMetadata,
        message_type: String,
        body: BTreeMap<String, Value>,
    },
}

impl AisStreamMessage {
    pub fn metadata(&self) -> &AisMetadata {
        match self {
            Self::UnknownMessage { metadata, .. }
            | Self::ShipStaticData { metadata, .. }
            | Self::StandardClassBPositionReport { metadata, .. }
            | Self::PositionReport { metadata, .. }
            | Self::Other { metadata, .. } => metadata,
        }
    }

    pub fn message_type(&self) -> &str {
        match self {
            Self::UnknownMessage { .. } => "UnknownMessage",
            Self::ShipStaticData { .. } => "ShipStaticData",
            Self::StandardClassBPositionReport { .. } => "StandardClassBPositionReport",
            Self::PositionReport { .. } => "PositionReport",
            Self::Other { message_type, .. } => message_type,
        }
    }

    fn to_raw_message(&self) -> BTreeMap<String, Value> {
        match self {
            Self::UnknownMessage { body, .. } => wrap_body("UnknownMessage", body),
            Self::ShipStaticData { body, .. } => wrap_body("ShipStaticData", body),
            Self::StandardClassBPositionReport { body, .. } => {
                wrap_body("StandardClassBPositionReport", body)
            }
            Self::PositionReport { body, .. } => wrap_body("PositionReport", body),
            Self::Other { body, .. } => body.clone(),
        }
    }
}

impl Serialize for AisStreamMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct("AisStreamMessage", 3)?;
        state.serialize_field("Message", &self.to_raw_message())?;
        state.serialize_field("MessageType", self.message_type())?;
        state.serialize_field("MetaData", self.metadata())?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for AisStreamMessage {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = RawAisStreamMessage::deserialize(deserializer)?;
        from_raw_message(raw).map_err(serde::de::Error::custom)
    }
}

fn from_raw_message(raw: RawAisStreamMessage) -> Result<AisStreamMessage, serde_json::Error> {
    let RawAisStreamMessage {
        message,
        message_type,
        metadata,
    } = raw;

    Ok(match message_type.as_str() {
        "UnknownMessage" => AisStreamMessage::UnknownMessage {
            metadata,
            body: serde_json::from_value(extract_body(message, "UnknownMessage"))?,
        },
        "ShipStaticData" => AisStreamMessage::ShipStaticData {
            metadata,
            body: serde_json::from_value(extract_body(message, "ShipStaticData"))?,
        },
        "StandardClassBPositionReport" => AisStreamMessage::StandardClassBPositionReport {
            metadata,
            body: serde_json::from_value(extract_body(message, "StandardClassBPositionReport"))?,
        },
        "PositionReport" => AisStreamMessage::PositionReport {
            metadata,
            body: serde_json::from_value(extract_body(message, "PositionReport"))?,
        },
        _ => AisStreamMessage::Other {
            metadata,
            message_type,
            body: message,
        },
    })
}

#[derive(Deserialize)]
struct RawAisStreamMessage {
    #[serde(rename = "Message")]
    message: BTreeMap<String, Value>,
    #[serde(rename = "MessageType")]
    message_type: String,
    #[serde(rename = "MetaData")]
    metadata: AisMetadata,
}

fn extract_body(mut raw_message: BTreeMap<String, Value>, key: &str) -> Value {
    raw_message
        .remove(key)
        .unwrap_or(Value::Object(Default::default()))
}

fn wrap_body<T: Serialize>(key: &str, body: &T) -> BTreeMap<String, Value> {
    let mut message = BTreeMap::new();
    message.insert(
        key.to_string(),
        serde_json::to_value(body).unwrap_or(Value::Object(Default::default())),
    );
    message
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct UnknownMessage {}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShipStaticData {
    #[serde(rename = "AisVersion")]
    pub ais_version: u8,
    #[serde(rename = "CallSign")]
    pub call_sign: String,
    #[serde(rename = "Destination")]
    pub destination: String,
    #[serde(rename = "Dimension")]
    pub dimension: ShipDimensions,
    #[serde(rename = "Dte")]
    pub dte: bool,
    #[serde(rename = "Eta")]
    pub eta: Eta,
    #[serde(rename = "FixType")]
    pub fix_type: u8,
    #[serde(rename = "ImoNumber")]
    pub imo_number: u32,
    #[serde(rename = "MaximumStaticDraught")]
    pub maximum_static_draught: f64,
    #[serde(rename = "MessageID")]
    pub message_id: u8,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "RepeatIndicator")]
    pub repeat_indicator: u8,
    #[serde(rename = "Spare")]
    pub spare: bool,
    #[serde(rename = "Type")]
    pub ship_type: u8,
    #[serde(rename = "UserID")]
    pub user_id: u64,
    #[serde(rename = "Valid")]
    pub valid: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ShipDimensions {
    #[serde(rename = "A")]
    pub a: u16,
    #[serde(rename = "B")]
    pub b: u16,
    #[serde(rename = "C")]
    pub c: u16,
    #[serde(rename = "D")]
    pub d: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Eta {
    #[serde(rename = "Day")]
    pub day: u8,
    #[serde(rename = "Hour")]
    pub hour: u8,
    #[serde(rename = "Minute")]
    pub minute: u8,
    #[serde(rename = "Month")]
    pub month: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StandardClassBPositionReport {
    #[serde(rename = "AssignedMode")]
    pub assigned_mode: bool,
    #[serde(rename = "ClassBBand")]
    pub class_b_band: bool,
    #[serde(rename = "ClassBDisplay")]
    pub class_b_display: bool,
    #[serde(rename = "ClassBDsc")]
    pub class_b_dsc: bool,
    #[serde(rename = "ClassBMsg22")]
    pub class_b_msg22: bool,
    #[serde(rename = "ClassBUnit")]
    pub class_b_unit: bool,
    #[serde(rename = "Cog")]
    pub cog: f64,
    #[serde(rename = "CommunicationState")]
    pub communication_state: u32,
    #[serde(rename = "CommunicationStateIsItdma")]
    pub communication_state_is_itdma: bool,
    #[serde(rename = "Latitude")]
    pub latitude: f64,
    #[serde(rename = "Longitude")]
    pub longitude: f64,
    #[serde(rename = "MessageID")]
    pub message_id: u8,
    #[serde(rename = "PositionAccuracy")]
    pub position_accuracy: bool,
    #[serde(rename = "Raim")]
    pub raim: bool,
    #[serde(rename = "RepeatIndicator")]
    pub repeat_indicator: u8,
    #[serde(rename = "Sog")]
    pub sog: f64,
    #[serde(rename = "Spare1")]
    pub spare1: u8,
    #[serde(rename = "Spare2")]
    pub spare2: u8,
    #[serde(rename = "Timestamp")]
    pub timestamp: u8,
    #[serde(rename = "TrueHeading")]
    pub true_heading: u16,
    #[serde(rename = "UserID")]
    pub user_id: u64,
    #[serde(rename = "Valid")]
    pub valid: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PositionReport {
    #[serde(rename = "Cog")]
    pub cog: f64,
    #[serde(rename = "CommunicationState")]
    pub communication_state: u32,
    #[serde(rename = "Latitude")]
    pub latitude: f64,
    #[serde(rename = "Longitude")]
    pub longitude: f64,
    #[serde(rename = "MessageID")]
    pub message_id: u8,
    #[serde(rename = "NavigationalStatus")]
    pub navigational_status: u8,
    #[serde(rename = "PositionAccuracy")]
    pub position_accuracy: bool,
    #[serde(rename = "Raim")]
    pub raim: bool,
    #[serde(rename = "RateOfTurn")]
    pub rate_of_turn: i16,
    #[serde(rename = "RepeatIndicator")]
    pub repeat_indicator: u8,
    #[serde(rename = "Sog")]
    pub sog: f64,
    #[serde(rename = "Spare")]
    pub spare: u8,
    #[serde(rename = "SpecialManoeuvreIndicator")]
    pub special_manoeuvre_indicator: u8,
    #[serde(rename = "Timestamp")]
    pub timestamp: u8,
    #[serde(rename = "TrueHeading")]
    pub true_heading: u16,
    #[serde(rename = "UserID")]
    pub user_id: u64,
    #[serde(rename = "Valid")]
    pub valid: bool,
}
