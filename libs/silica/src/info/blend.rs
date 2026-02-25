#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlendingMode {
    Normal = 0,
    Multiply = 1,
    Screen = 2,
    Add = 3,
    Lighten = 4,
    Exclusion = 5,
    Difference = 6,
    Subtract = 7,
    LinearBurn = 8,
    ColorDodge = 9,
    ColorBurn = 10,
    Overlay = 11,
    HardLight = 12,
    Color = 13,
    Luminosity = 14,
    Hue = 15,
    Saturation = 16,
    SoftLight = 17,
    Darken = 19,
    HardMix = 20,
    VividLight = 21,
    LinearLight = 22,
    PinLight = 23,
    LighterColor = 24,
    DarkerColor = 25,
    Divide = 26,
}

impl std::fmt::Display for BlendingMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl BlendingMode {
    pub fn all() -> &'static [BlendingMode] {
        use BlendingMode::*;
        &[
            Normal,
            Multiply,
            Screen,
            Add,
            Lighten,
            Exclusion,
            Difference,
            Subtract,
            LinearBurn,
            ColorDodge,
            ColorBurn,
            Overlay,
            HardLight,
            Color,
            Luminosity,
            Hue,
            Saturation,
            SoftLight,
            Darken,
            HardMix,
            VividLight,
            LinearLight,
            PinLight,
            LighterColor,
            DarkerColor,
            Divide,
        ]
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Add => "Add",
            Self::Lighten => "Lighten",
            Self::Exclusion => "Exclusion",
            Self::Difference => "Difference",
            Self::Subtract => "Subtract",
            Self::LinearBurn => "Linear Burn",
            Self::ColorDodge => "Color Dodge",
            Self::ColorBurn => "Color Burn",
            Self::Overlay => "Overlay",
            Self::HardLight => "Hard Light",
            Self::Color => "Color",
            Self::Luminosity => "Luminosity",
            Self::Hue => "Hue",
            Self::Saturation => "Saturation",
            Self::SoftLight => "Soft Light",
            Self::Darken => "Darken",
            Self::HardMix => "Hard Mix",
            Self::VividLight => "Vivid Light",
            Self::LinearLight => "Linear Light",
            Self::PinLight => "Pin Light",
            Self::LighterColor => "Lighter Color",
            Self::DarkerColor => "Darker Color",
            Self::Divide => "Divide",
        }
    }

    pub fn from_u32(blend: u32) -> Option<Self> {
        Some(match blend {
            0 => Self::Normal,
            1 => Self::Multiply,
            2 => Self::Screen,
            3 => Self::Add,
            4 => Self::Lighten,
            5 => Self::Exclusion,
            6 => Self::Difference,
            7 => Self::Subtract,
            8 => Self::LinearBurn,
            9 => Self::ColorDodge,
            10 => Self::ColorBurn,
            11 => Self::Overlay,
            12 => Self::HardLight,
            13 => Self::Color,
            14 => Self::Luminosity,
            15 => Self::Hue,
            16 => Self::Saturation,
            17 => Self::SoftLight,
            19 => Self::Darken,
            20 => Self::HardMix,
            21 => Self::VividLight,
            22 => Self::LinearLight,
            23 => Self::PinLight,
            24 => Self::LighterColor,
            25 => Self::DarkerColor,
            26 => Self::Divide,
            _ => return None,
        })
    }

    pub fn to_u32(self) -> u32 {
        self as u32
    }
}
