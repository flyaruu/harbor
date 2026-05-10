use spacetimedb::SpacetimeType;

#[derive(SpacetimeType, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MajorAisShipType {
    NotAvailable,
    Reserved,
    WingInGround,
    FishingOrService,
    HighSpeedCraft,
    SpecialCraft,
    Passenger,
    Cargo,
    Tanker,
    Other,
    Unknown(u8),
}

impl From<u8> for MajorAisShipType {
    fn from(code: u8) -> Self {
        match code {
            0 => Self::NotAvailable,
            1..=19 => Self::Reserved,
            20..=29 => Self::WingInGround,
            30..=39 => Self::FishingOrService,
            40..=49 => Self::HighSpeedCraft,
            50..=59 => Self::SpecialCraft,
            60..=69 => Self::Passenger,
            70..=79 => Self::Cargo,
            80..=89 => Self::Tanker,
            90..=99 => Self::Other,
            other => Self::Unknown(other),
        }
    }
}

impl MajorAisShipType {
    pub fn describe(&self) -> &'static str {
        match self {
            Self::NotAvailable => "Not available or no ship type reported",
            Self::Reserved => "Reserved AIS ship type code",
            Self::WingInGround => "Wing in ground craft or search and rescue aircraft",
            Self::FishingOrService => {
                "Fishing, towing, dredging, diving, military, sailing, or pleasure craft"
            }
            Self::HighSpeedCraft => "High-speed craft",
            Self::SpecialCraft => {
                "Special craft such as pilot vessel, SAR vessel, tug, port tender, law enforcement, or medical transport"
            }
            Self::Passenger => "Passenger vessel",
            Self::Cargo => "Cargo vessel",
            Self::Tanker => "Tanker",
            Self::Other => "Other ship or cargo type",
            Self::Unknown(_) => "Unknown AIS ship type code",
        }
    }

    pub fn short_description(&self) -> &'static str {
        match self {
            Self::NotAvailable => "Not available",
            Self::Reserved => "Reserved",
            Self::WingInGround => "WIG / SAR aircraft",
            Self::FishingOrService => "Fishing / service",
            Self::HighSpeedCraft => "High-speed craft",
            Self::SpecialCraft => "Special craft",
            Self::Passenger => "Passenger",
            Self::Cargo => "Cargo",
            Self::Tanker => "Tanker",
            Self::Other => "Other",
            Self::Unknown(_) => "Unknown",
        }
    }
}
