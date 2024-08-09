//! Color util methods.
//!
//! Converted from https://github.com/home-assistant/core/blob/dev/homeassistant/util/color.py
//! Apache-2.0 license

use derive_more::Constructor;

/// Represents a CIE 1931 XY coordinate pair.
#[derive(Constructor, Clone, Copy)]
pub struct XYPoint {
    pub x: f32,
    pub y: f32,
}

/// Represents the Gamut of a light.
#[derive(Constructor, Clone, Copy)]
pub struct GamutType {
    // ColorGamut = gamut(XYPoint::new(xR,yR),XYPoint::new(xG,yG),XYPoint::new(xB,yB))
    pub red: XYPoint,
    pub green: XYPoint,
    pub blue: XYPoint,
}

/// Convert from XY to a normalized RGB.
pub fn color_xy_to_rgb(x: f32, y: f32, gamut: Option<GamutType>) -> (u16, u16, u16) {
    color_xy_brightness_to_rgb(x, y, 255, gamut)
}

/// Convert from XYZ to RGB.
// Converted to Rust from Python from Obj-C, original source from:
// https://github.com/PhilipsHue/PhilipsHueSDK-iOS-OSX/blob/00187a3/ApplicationDesignNotes/RGB%20to%20xy%20Color%20conversion.md
pub fn color_xy_brightness_to_rgb(
    mut v_x: f32,
    mut v_y: f32,
    ibrightness: u16,
    gamut: Option<GamutType>,
) -> (u16, u16, u16) {
    if let Some(gamut) = gamut {
        if !check_point_in_lamps_reach((v_x, v_y), gamut) {
            let xy_closest = get_closest_point_to_point((v_x, v_y), gamut);
            v_x = xy_closest.0;
            v_y = xy_closest.1;
        }
    }

    let brightness = ibrightness as f32 / 255.0;
    if brightness == 0.0 {
        return (0, 0, 0);
    }

    let y = brightness;

    if v_y == 0.0 {
        v_y += 0.00000000001;
    }

    let x = (y / v_y) * v_x;
    let z = (y / v_y) * (1_f32 - v_x - v_y);

    // Convert to RGB using Wide RGB D65 conversion.
    let mut r = x * 1.656492 - y * 0.354851 - z * 0.255038;
    let mut g = -x * 0.707196 + y * 1.655397 + z * 0.036152;
    let mut b = x * 0.051713 - y * 0.121364 + z * 1.01153;

    // Apply reverse gamma correction.
    fn reverse_gamma(x: f32) -> f32 {
        if x <= 0.0031308 {
            12.92 * x
        } else {
            (1.0 + 0.055) * x.powf(1.0 / 2.4) - 0.055
        }
    }
    r = reverse_gamma(r);
    g = reverse_gamma(g);
    b = reverse_gamma(b);

    // Bring all negative components to zero.
    r = r.max(0.);
    g = g.max(0.);
    b = b.max(0.);

    // If one component is greater than 1, weight components by that value.
    let max_component = r.max(g).max(b);
    if max_component > 1_f32 {
        r /= max_component;
        g /= max_component;
        b /= max_component;
    }

    ((r * 255.) as u16, (g * 255.) as u16, (b * 255.) as u16)
}

/// Convert an rgb color to its hsv representation.
///
/// - Hue is scaled 0-360
/// - Sat is scaled 0-100
/// - Val is scaled 0-100
pub fn color_rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let (h, s, v) = rgb_to_hsv(r / 255.0, g / 255.0, b / 255.0);
    (round(h * 360., 3), round(s * 100., 3), round(v * 100., 3))
}

/// Convert an xy color to its hs representation.
pub fn color_xy_to_hs(x: f32, y: f32, gamut: Option<GamutType>) -> (f32, f32) {
    let (r, g, b) = color_xy_to_rgb(x, y, gamut);
    let (h, s, _) = color_rgb_to_hsv(r as f32, g as f32, b as f32);
    (h, s)
}

// The following 5 functions are adapted from rgbxy provided by Benjamin Knight
// License: The MIT License (MIT), 2014.
// https://github.com/benknight/hue-python-rgb-converter

/// Calculate the cross product of two XYPoints.
fn cross_product(p1: XYPoint, p2: XYPoint) -> f32 {
    p1.x * p2.y - p1.y * p2.x
}

/// Calculate the distance between two XYPoints.
fn get_distance_between_two_points(one: XYPoint, two: XYPoint) -> f32 {
    let dx = one.x - two.x;
    let dy = one.y - two.y;

    (dx * dx + dy * dy).sqrt()
}

/// Find the closest point from P to a line defined by A and B.
///
/// This point will be reproducible by the lamp
/// as it is on the edge of the gamut.
fn get_closest_point_to_line(a: XYPoint, b: XYPoint, p: XYPoint) -> XYPoint {
    let ap = XYPoint::new(p.x - a.x, p.y - a.y);
    let ab = XYPoint::new(b.x - a.x, b.y - a.y);
    let ab2 = ab.x * ab.x + ab.y * ab.y;
    let ap_ab = ap.x * ab.x + ap.y * ab.y;
    let mut t = ap_ab / ab2;

    t = t.clamp(0.0, 1.0);

    XYPoint::new(a.x + ab.x * t, a.y + ab.y * t)
}

/// Get the closest matching color within the gamut of the light.
///
/// Should only be used if the supplied color is outside of the color gamut.
fn get_closest_point_to_point(xy_tuple: (f32, f32), gamut: GamutType) -> (f32, f32) {
    let xy_point = XYPoint::new(xy_tuple.0, xy_tuple.1);

    // find the closest point on each line in the CIE 1931 'triangle'.
    let p_ab = get_closest_point_to_line(gamut.red, gamut.green, xy_point);
    let p_ac = get_closest_point_to_line(gamut.blue, gamut.red, xy_point);
    let p_bc = get_closest_point_to_line(gamut.green, gamut.blue, xy_point);

    // Get the distances per point and see which point is closer to our Point.
    let d_ab = get_distance_between_two_points(xy_point, p_ab);
    let d_ac = get_distance_between_two_points(xy_point, p_ac);
    let d_bc = get_distance_between_two_points(xy_point, p_bc);

    let mut lowest = d_ab;
    let mut closest_point = p_ab;

    if d_ac < lowest {
        lowest = d_ac;
        closest_point = p_ac;
    }

    if d_bc < lowest {
        // lowest = dBC;
        closest_point = p_bc;
    }

    // Change the xy value to a value which is within the reach of the lamp.
    let cx = closest_point.x;
    let cy = closest_point.y;
    (cx, cy)
}

/// Check if the provided XYPoint can be recreated by a Hue lamp.
fn check_point_in_lamps_reach(p: (f32, f32), gamut: GamutType) -> bool {
    let v1 = XYPoint::new(gamut.green.x - gamut.red.x, gamut.green.y - gamut.red.y);
    let v2 = XYPoint::new(gamut.blue.x - gamut.red.x, gamut.blue.y - gamut.red.y);

    let q = XYPoint::new(p.0 - gamut.red.x, p.1 - gamut.red.y);
    let s = cross_product(q, v2) / cross_product(v1, v2);
    let t = cross_product(v1, q) / cross_product(v1, v2);

    (s >= 0.0) && (t >= 0.0) && (s + t <= 1.0)
}

// HSV: Hue, Saturation, Value
// H: position in the spectrum
// S: color saturation ("purity")
// V: color brightness
// From: https://github.com/python/cpython/blob/3.12/Lib/colorsys.py

pub fn rgb_to_hsv(r: f32, g: f32, b: f32) -> (f32, f32, f32) {
    let maxc = r.max(g).max(b);
    let minc = r.min(g).min(b);
    let rangec = maxc - minc;
    let v = maxc;

    if minc == maxc {
        return (0.0, 0.0, v);
    }
    let s = rangec / maxc;
    let rc = (maxc - r) / rangec;
    let gc = (maxc - g) / rangec;
    let bc = (maxc - b) / rangec;
    let mut h = if r == maxc {
        bc - gc
    } else if g == maxc {
        2.0 + rc - bc
    } else {
        4.0 + gc - rc
    };

    // Modulo operation: in Python the remainder will take the sign of the divisor, in Rust it will take the sign of the dividend
    // h = (h / 6.0) % 1.0;
    h = (h / 6.0).rem_euclid(1.0);
    (h, s, v)
}

fn round(x: f32, decimals: u32) -> f32 {
    let y = 10i32.pow(decimals) as f32;
    (x * y).round() / y
}

#[cfg(test)]
mod tests {
    use super::*;
    use lazy_static::lazy_static;
    use rstest::rstest;

    lazy_static! {
        static ref GAMUT: GamutType = GamutType::new(
            XYPoint::new(0.704, 0.296),
            XYPoint::new(0.2151, 0.7106),
            XYPoint::new(0.138, 0.08),
        );
    }

    #[rstest]
    #[case((0, 0, 0), 1., 1., 0, None)]
    #[case((194, 186, 169), 0.35, 0.35, 128, None)]
    #[case((255, 243, 222), 0.35, 0.35, 255, None)]
    #[case((255, 0, 60), 1., 0., 255, None)]
    #[case((0, 255, 0), 0., 1., 255, None)]
    #[case((0, 63, 255), 0., 0., 255, None)]
    #[case((255, 0, 3), 1., 0., 255, Some(*GAMUT))]
    #[case((82, 255, 0), 0., 1., 255, Some(*GAMUT))]
    #[case((9, 85, 255), 0., 0., 255, Some(*GAMUT))]
    fn test_color_xy_brightness_to_rgb(
        #[case] expected: (u16, u16, u16),
        #[case] x: f32,
        #[case] y: f32,
        #[case] brightness: u16,
        #[case] gamut: Option<GamutType>,
    ) {
        assert_eq!(
            expected,
            color_xy_brightness_to_rgb(x, y, brightness, gamut)
        );
    }

    #[rstest]
    #[case((255, 243, 222), 0.35, 0.35, None)]
    #[case((255, 0, 60), 1., 0., None)]
    #[case((0, 255, 0), 0., 1., None)]
    #[case((0, 63, 255), 0., 0., None)]
    #[case((255, 0, 3), 1., 0., Some(*GAMUT))]
    #[case((82, 255, 0), 0., 1., Some(*GAMUT))]
    #[case((9, 85, 255), 0., 0., Some(*GAMUT))]
    fn test_color_xy_to_rgb(
        #[case] expected: (u16, u16, u16),
        #[case] x: f32,
        #[case] y: f32,
        #[case] gamut: Option<GamutType>,
    ) {
        assert_eq!(expected, color_xy_to_rgb(x, y, gamut));
    }

    #[rstest]
    #[case((0., 0., 0.), 0., 0., 0.)]
    #[case((0., 0., 100.), 255., 255., 255.)]
    #[case((240., 100., 100.), 0., 0., 255.)]
    #[case((120., 100., 100.), 0., 255., 0.)]
    #[case((0., 100., 100.), 255., 0., 0.)]
    fn test_color_rgb_to_hsv(
        #[case] expected: (f32, f32, f32),
        #[case] r: f32,
        #[case] g: f32,
        #[case] b: f32,
    ) {
        assert_eq!(expected, color_rgb_to_hsv(r, g, b));
    }

    #[rstest]
    #[case((47.294, 100.), 1., 1., None)]
    #[case((38.182, 12.941), 0.35, 0.35, None)]
    #[case((345.882, 100.), 1., 0., None)]
    #[case((120., 100.), 0., 1., None)]
    #[case((225.176, 100.), 0., 0., None)]
    #[case((359.294, 100.), 1., 0., Some(*GAMUT))]
    #[case((100.706, 100.), 0., 1., Some(*GAMUT))]
    #[case((221.463, 96.471), 0., 0., Some(*GAMUT))]
    fn test_color_xy_to_hs(
        #[case] expected: (f32, f32),
        #[case] x: f32,
        #[case] y: f32,
        #[case] gamut: Option<GamutType>,
    ) {
        assert_eq!(expected, color_xy_to_hs(x, y, gamut));
    }
}
