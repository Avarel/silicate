use silica_gpu as data;
use silicate_compositor::blend as comp;

pub fn convert_blend(blend: data::BlendingMode) -> comp::BlendingMode {
    match blend {
        data::BlendingMode::Normal => comp::BlendingMode::Normal,
        data::BlendingMode::Multiply => comp::BlendingMode::Multiply,
        data::BlendingMode::Screen => comp::BlendingMode::Screen,
        data::BlendingMode::Add => comp::BlendingMode::Add,
        data::BlendingMode::Lighten => comp::BlendingMode::Lighten,
        data::BlendingMode::Exclusion => comp::BlendingMode::Exclusion,
        data::BlendingMode::Difference => comp::BlendingMode::Difference,
        data::BlendingMode::Subtract => comp::BlendingMode::Subtract,
        data::BlendingMode::LinearBurn => comp::BlendingMode::LinearBurn,
        data::BlendingMode::ColorDodge => comp::BlendingMode::ColorDodge,
        data::BlendingMode::ColorBurn => comp::BlendingMode::ColorBurn,
        data::BlendingMode::Overlay => comp::BlendingMode::Overlay,
        data::BlendingMode::HardLight => comp::BlendingMode::HardLight,
        data::BlendingMode::Color => comp::BlendingMode::Color,
        data::BlendingMode::Luminosity => comp::BlendingMode::Luminosity,
        data::BlendingMode::Hue => comp::BlendingMode::Hue,
        data::BlendingMode::Saturation => comp::BlendingMode::Saturation,
        data::BlendingMode::SoftLight => comp::BlendingMode::SoftLight,
        data::BlendingMode::Darken => comp::BlendingMode::Darken,
        data::BlendingMode::HardMix => comp::BlendingMode::HardMix,
        data::BlendingMode::VividLight => comp::BlendingMode::VividLight,
        data::BlendingMode::LinearLight => comp::BlendingMode::LinearLight,
        data::BlendingMode::PinLight => comp::BlendingMode::PinLight,
        data::BlendingMode::LighterColor => comp::BlendingMode::LighterColor,
        data::BlendingMode::DarkerColor => comp::BlendingMode::DarkerColor,
        data::BlendingMode::Divide => comp::BlendingMode::Divide,
    }
}
