pub fn comp(cv: f32, alpha: f32) -> f32 {
    cv * (1.0 - alpha)
}

pub fn normal(c1: f32, c2: f32, _: f32, a2: f32) -> f32 {
    c2 + comp(c1, a2)
}

pub fn multiply(c1: f32, c2: f32, a1: f32, a2: f32) -> f32 {
    c2 * c1 + comp(c2, a1) + comp(c1, a2)
}

// works great!
pub fn screen(c1: f32, c2: f32, _: f32, _: f32) -> f32 {
    c2 + c1 - c2 * c1
}

// works great!
pub fn overlay(c1: f32, c2: f32, a1: f32, a2: f32) -> f32 {
    if c1 * 2.0 <= a1 {
        c2 * c1 * 2.0 + comp(c2, a1) + comp(c1, a2)
    } else {
        comp(c2, a1) + comp(c1, a2) - 2.0 * (a1 - c1) * (a2 - c2) + a2 * a1
    }
}

pub type BlendingFunction = fn(f32, f32, f32, f32) -> f32;