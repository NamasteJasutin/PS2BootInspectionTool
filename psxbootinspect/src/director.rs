//! Director mode: camera scripts as keyframed data, seeded montage shuffle, and console
//! homages expressed as motion-only scripts.

#![allow(dead_code)]

use crate::renderer::ViewCamera;
use glam::Vec3;
use ps2kit::VideoMode;

/// Interpolation easing applied across keyframe intervals.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Easing {
    /// Constant-speed linear interpolation.
    Linear,
    /// Smooth cosine-based acceleration and deceleration.
    EaseInOut,
    /// Holds the previous key's state until the next keyframe.
    Hold,
}

impl Easing {
    /// String identifier in JSON representations.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::EaseInOut => "ease-in-out",
            Self::Hold => "hold",
        }
    }

    /// Parses an easing name from JSON.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "linear" => Some(Self::Linear),
            "ease-in-out" | "ease_in_out" | "easeInOut" => Some(Self::EaseInOut),
            "hold" => Some(Self::Hold),
            _ => None,
        }
    }

    /// Evaluates the normalized time progress `u` in `[0, 1]`.
    pub fn ease(self, u: f32) -> f32 {
        let u = u.clamp(0.0, 1.0);
        match self {
            Self::Linear => u,
            Self::EaseInOut => (1.0 - (u * std::f32::consts::PI).cos()) * 0.5,
            Self::Hold => {
                if u >= 1.0 {
                    1.0
                } else {
                    0.0
                }
            }
        }
    }
}

/// A keyframe in the scene's normalised frame.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Key {
    /// Timeline frame on which this key lands.
    pub frame: f32,
    /// Horizontal rotation angle in degrees around the vertical axis relative to `front`.
    pub azimuth_deg: f32,
    /// Elevation along the vertical axis above/below centre, as a fraction of `radius`.
    pub elevation: f32,
    /// Distance from the vertical axis, as a fraction of `radius`.
    pub distance: f32,
    /// Aim offset along the vertical axis from the path centre, as a fraction of `radius`
    /// (0 = the centre itself, which the app already places at the Orbit dial's focus height).
    pub aim: f32,
    /// Camera lens zoom relative to default console focal length (like `PathParams::zoom`).
    pub lens: f32,
    /// Camera roll angle in degrees around the line of sight.
    pub roll_deg: f32,
    /// Optional per-key transition easing to the next keyframe.
    pub easing: Option<Easing>,
}

impl Key {
    /// Creates a keyframe with default (script-level) easing.
    pub const fn new(
        frame: f32,
        azimuth_deg: f32,
        elevation: f32,
        distance: f32,
        aim: f32,
        lens: f32,
        roll_deg: f32,
    ) -> Self {
        Self {
            frame,
            azimuth_deg,
            elevation,
            distance,
            aim,
            lens,
            roll_deg,
            easing: None,
        }
    }

    /// Overrides the interval easing starting from this keyframe.
    pub const fn with_easing(mut self, easing: Easing) -> Self {
        self.easing = Some(easing);
        self
    }
}

/// A keyframed camera script that can be played, edited, saved, or loaded.
#[derive(Clone, PartialEq, Debug)]
pub struct CameraScript {
    /// Human-readable name of the script.
    pub name: String,
    /// Ordered list of keyframes.
    pub keys: Vec<Key>,
    /// Whether the camera path wraps around and loops continuously.
    pub looped: bool,
    /// Default fallback easing for key intervals where no per-key easing is set.
    pub easing: Easing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct KeyParams {
    azimuth: f32,
    elevation: f32,
    distance: f32,
    aim: f32,
    lens: f32,
    roll: f32,
}

impl KeyParams {
    fn from_key(k: &Key) -> Self {
        Self {
            azimuth: k.azimuth_deg,
            elevation: k.elevation,
            distance: k.distance,
            aim: k.aim,
            lens: k.lens,
            roll: k.roll_deg,
        }
    }

    fn add(self, rhs: Self) -> Self {
        Self {
            azimuth: self.azimuth + rhs.azimuth,
            elevation: self.elevation + rhs.elevation,
            distance: self.distance + rhs.distance,
            aim: self.aim + rhs.aim,
            lens: self.lens + rhs.lens,
            roll: self.roll + rhs.roll,
        }
    }

    fn scale(self, s: f32) -> Self {
        Self {
            azimuth: self.azimuth * s,
            elevation: self.elevation * s,
            distance: self.distance * s,
            aim: self.aim * s,
            lens: self.lens * s,
            roll: self.roll * s,
        }
    }

    fn knot_dist(p0: &Self, p1: &Self) -> f32 {
        let d2 = (p1.azimuth - p0.azimuth).powi(2)
            + (p1.elevation - p0.elevation).powi(2)
            + (p1.distance - p0.distance).powi(2)
            + (p1.aim - p0.aim).powi(2)
            + (p1.lens - p0.lens).powi(2)
            + (p1.roll - p0.roll).powi(2);
        // Centripetal Catmull-Rom parameter: alpha = 0.5 => knot interval = d2^(0.25).
        d2.sqrt().powf(0.5).max(1e-4)
    }
}

fn catmull_rom_centripetal(
    p0: KeyParams,
    p1: KeyParams,
    p2: KeyParams,
    p3: KeyParams,
    t: f32,
) -> KeyParams {
    if t <= 0.0 {
        return p1;
    }
    if t >= 1.0 {
        return p2;
    }

    let dt01 = KeyParams::knot_dist(&p0, &p1);
    let dt12 = KeyParams::knot_dist(&p1, &p2);
    let dt23 = KeyParams::knot_dist(&p2, &p3);

    let t0 = 0.0f32;
    let t1 = t0 + dt01;
    let t2 = t1 + dt12;
    let t3 = t2 + dt23;

    let tau = t1 + t * dt12;

    let lerp = |a: KeyParams, b: KeyParams, ta: f32, tb: f32| -> KeyParams {
        let denom = tb - ta;
        if denom.abs() < 1e-6 {
            a
        } else {
            a.scale((tb - tau) / denom).add(b.scale((tau - ta) / denom))
        }
    };

    let a1 = lerp(p0, p1, t0, t1);
    let a2 = lerp(p1, p2, t1, t2);
    let a3 = lerp(p2, p3, t2, t3);

    let b1 = lerp(a1, a2, t0, t2);
    let b2 = lerp(a2, a3, t1, t3);

    lerp(b1, b2, t1, t2)
}

impl CameraScript {
    /// Creates a new camera script.
    pub fn new(name: impl Into<String>, keys: Vec<Key>, looped: bool, easing: Easing) -> Self {
        let mut s = Self {
            name: name.into(),
            keys,
            looped,
            easing,
        };
        s.sort_keys();
        s
    }

    /// Sorts keyframes chronologically by frame number.
    pub fn sort_keys(&mut self) {
        self.keys.sort_by(|a, b| a.frame.partial_cmp(&b.frame).unwrap_or(std::cmp::Ordering::Equal));
    }

    /// Total timeline span covered by keyframes (0.0 if empty or single key).
    pub fn duration(&self) -> f32 {
        if self.keys.len() < 2 {
            0.0
        } else {
            self.keys.last().unwrap().frame - self.keys.first().unwrap().frame
        }
    }

    /// Evaluates the script at `frame`, relative to scene normalised frame parameters.
    ///
    /// Applies centripetal Catmull-Rom spline interpolation, per-key easing, and the exact
    /// screen-down convention from [`crate::model::CameraMode::view`].
    pub fn view(
        &self,
        frame: f32,
        centre: Vec3,
        radius: f32,
        vertical: Vec3,
        front: Vec3,
        video: VideoMode,
    ) -> Option<ViewCamera> {
        if self.keys.is_empty() {
            return None;
        }
        if self.keys.len() == 1 {
            return Some(self.render_params(
                &KeyParams::from_key(&self.keys[0]),
                centre,
                radius,
                vertical,
                front,
                video,
            ));
        }

        let first_frame = self.keys.first().unwrap().frame;
        let last_frame = self.keys.last().unwrap().frame;
        let span = last_frame - first_frame;

        let query_frame = if self.looped && span > 0.0 {
            first_frame + (frame - first_frame).rem_euclid(span)
        } else {
            frame
        };

        if query_frame <= first_frame {
            return Some(self.render_params(
                &KeyParams::from_key(self.keys.first().unwrap()),
                centre,
                radius,
                vertical,
                front,
                video,
            ));
        }
        if query_frame >= last_frame {
            return Some(self.render_params(
                &KeyParams::from_key(self.keys.last().unwrap()),
                centre,
                radius,
                vertical,
                front,
                video,
            ));
        }

        // Find keyframe interval i .. i+1
        let n = self.keys.len();
        let mut idx = 0;
        for i in 0..n - 1 {
            if query_frame >= self.keys[i].frame && query_frame <= self.keys[i + 1].frame {
                idx = i;
                break;
            }
        }

        let k1 = &self.keys[idx];
        let k2 = &self.keys[idx + 1];
        let seg_len = k2.frame - k1.frame;
        let u = if seg_len > 0.0 {
            (query_frame - k1.frame) / seg_len
        } else {
            0.0
        };

        let easing = k1.easing.unwrap_or(self.easing);
        let t = easing.ease(u);

        // Control points for Catmull-Rom
        let p1 = KeyParams::from_key(k1);
        let p2 = KeyParams::from_key(k2);

        let p0 = if idx > 0 {
            KeyParams::from_key(&self.keys[idx - 1])
        } else if self.looped && n > 2 {
            // In a looped path, previous key wraps around
            let prev = KeyParams::from_key(&self.keys[n - 2]);
            KeyParams {
                azimuth: prev.azimuth - 360.0,
                ..prev
            }
        } else {
            // Clamped endpoint tangent: 2*P1 - P2
            p1.scale(2.0).add(p2.scale(-1.0))
        };

        let p3 = if idx + 2 < n {
            KeyParams::from_key(&self.keys[idx + 2])
        } else if self.looped && n > 2 {
            let next = KeyParams::from_key(&self.keys[1]);
            KeyParams {
                azimuth: next.azimuth + 360.0,
                ..next
            }
        } else {
            p2.scale(2.0).add(p1.scale(-1.0))
        };

        let interpolated = catmull_rom_centripetal(p0, p1, p2, p3, t);

        Some(self.render_params(&interpolated, centre, radius, vertical, front, video))
    }

    fn render_params(
        &self,
        params: &KeyParams,
        centre: Vec3,
        radius: f32,
        vertical: Vec3,
        front: Vec3,
        video: VideoMode,
    ) -> ViewCamera {
        let down = vertical.normalize_or(Vec3::Y);
        let up = -down;
        let front = (front - down * front.dot(down)).normalize_or(Vec3::X);
        let side = down.cross(front).normalize_or(Vec3::X);
        let ring = |angle_rad: f32, r: f32| (front * angle_rad.cos() + side * angle_rad.sin()) * r;

        let a = params.azimuth.to_radians();
        let position = centre + up * (params.elevation * radius) + ring(a, params.distance * radius);

        let target = centre + up * (params.aim * radius);
        let forward = (target - position).normalize_or(down);

        // Screen-down rule matching CameraMode::view:
        let proj = down - forward * forward.dot(down);
        let screen_down = if proj.length() < 0.05 {
            (front - forward * forward.dot(front)).normalize_or(Vec3::Y)
        } else {
            proj.normalize()
        };

        let final_up = if params.roll.abs() > 1e-4 {
            let roll_rad = params.roll.to_radians();
            let screen_right = screen_down.cross(forward).normalize_or(Vec3::X);
            (screen_down * roll_rad.cos() + screen_right * roll_rad.sin()).normalize()
        } else {
            screen_down
        };

        ViewCamera {
            position,
            forward,
            up: final_up,
            video,
            zoom: params.lens.max(0.01),
        }
    }

    /// Serializes this camera script to a strict, formatted JSON string.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\n");
        out.push_str(&format!("  \"name\": \"{}\",\n", escape_json_str(&self.name)));
        out.push_str(&format!("  \"looped\": {},\n", self.looped));
        out.push_str(&format!("  \"easing\": \"{}\",\n", self.easing.as_str()));
        out.push_str("  \"keys\": [\n");
        for (i, k) in self.keys.iter().enumerate() {
            out.push_str("    {\n");
            out.push_str(&format!("      \"frame\": {},\n", fmt_f32(k.frame)));
            out.push_str(&format!("      \"azimuth_deg\": {},\n", fmt_f32(k.azimuth_deg)));
            out.push_str(&format!("      \"elevation\": {},\n", fmt_f32(k.elevation)));
            out.push_str(&format!("      \"distance\": {},\n", fmt_f32(k.distance)));
            out.push_str(&format!("      \"aim\": {},\n", fmt_f32(k.aim)));
            out.push_str(&format!("      \"lens\": {},\n", fmt_f32(k.lens)));
            out.push_str(&format!("      \"roll_deg\": {}", fmt_f32(k.roll_deg)));
            if let Some(e) = k.easing {
                out.push_str(&format!(",\n      \"easing\": \"{}\"", e.as_str()));
            }
            out.push('\n');
            if i + 1 < self.keys.len() {
                out.push_str("    },\n");
            } else {
                out.push_str("    }\n");
            }
        }
        out.push_str("  ]\n");
        out.push_str("}\n");
        out
    }

    /// Parses a camera script from a strict JSON string.
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        let val = JsonParser::parse(json_str)?;
        let obj = match val {
            JsonValue::Object(map) => map,
            _ => return Err("expected root JSON object".into()),
        };

        let get = |k: &str| -> Option<&JsonValue> {
            obj.iter().find(|(name, _)| name == k).map(|(_, v)| v)
        };

        let name = get("name")
            .and_then(|v| match v {
                JsonValue::String(s) => Some(s.clone()),
                _ => None,
            })
            .ok_or_else(|| "missing or non-string 'name' in script".to_string())?;

        let looped = get("looped")
            .and_then(|v| match v {
                JsonValue::Bool(b) => Some(*b),
                _ => None,
            })
            .unwrap_or(false);

        let easing = get("easing")
            .and_then(|v| match v {
                JsonValue::String(s) => Easing::parse(s),
                _ => None,
            })
            .unwrap_or(Easing::Linear);

        let keys_arr = get("keys")
            .and_then(|v| match v {
                JsonValue::Array(arr) => Some(arr),
                _ => None,
            })
            .ok_or_else(|| "missing or non-array 'keys' in script".to_string())?;

        let mut keys = Vec::with_capacity(keys_arr.len());
        for (idx, key_val) in keys_arr.iter().enumerate() {
            let key_obj = match key_val {
                JsonValue::Object(fields) => fields,
                _ => return Err(format!("key at index {idx} is not an object")),
            };

            let get_f32 = |field: &str, aliases: &[&str]| -> Result<f32, String> {
                for &k in std::iter::once(&field).chain(aliases.iter()) {
                    if let Some((_, v)) = key_obj.iter().find(|(f, _)| f == k) {
                        return match v {
                            JsonValue::Number(n) => Ok(*n as f32),
                            _ => Err(format!("field '{field}' in key {idx} is not a number")),
                        };
                    }
                }
                Err(format!("missing field '{field}' in key {idx}"))
            };

            let frame = get_f32("frame", &[])?;
            let azimuth_deg = get_f32("azimuth_deg", &["azimuth"])?;
            let elevation = get_f32("elevation", &[])?;
            let distance = get_f32("distance", &[])?;
            let aim = get_f32("aim", &[])?;
            let lens = get_f32("lens", &["zoom"])?;
            let roll_deg = get_f32("roll_deg", &["roll"])?;

            let key_easing = key_obj
                .iter()
                .find(|(f, _)| f == "easing")
                .and_then(|(_, v)| match v {
                    JsonValue::String(s) => Easing::parse(s),
                    _ => None,
                });

            keys.push(Key {
                frame,
                azimuth_deg,
                elevation,
                distance,
                aim,
                lens,
                roll_deg,
                easing: key_easing,
            });
        }

        let mut script = Self {
            name,
            keys,
            looped,
            easing,
        };
        script.sort_keys();
        Ok(script)
    }
}

// -----------------------------------------------------------------------------
// Small, strict JSON parser subset
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum JsonValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<JsonValue>),
    Object(Vec<(String, JsonValue)>),
}

struct JsonParser<'a> {
    src: &'a str,
    bytes: &'a [u8],
    pos: usize,
    depth: usize,
}

impl<'a> JsonParser<'a> {
    fn parse(src: &'a str) -> Result<JsonValue, String> {
        let mut p = Self {
            src,
            bytes: src.as_bytes(),
            pos: 0,
            depth: 0,
        };
        p.skip_whitespace();
        let val = p.parse_value()?;
        p.skip_whitespace();
        if p.pos < p.bytes.len() {
            let (l, c) = p.line_col();
            return Err(format!(
                "unexpected trailing character '{}' at line {l}, col {c}",
                p.bytes[p.pos] as char
            ));
        }
        Ok(val)
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.bytes.len() {
            match self.bytes[self.pos] {
                b' ' | b'\t' | b'\n' | b'\r' => self.pos += 1,
                _ => break,
            }
        }
    }

    fn line_col(&self) -> (usize, usize) {
        let mut line = 1;
        let mut col = 1;
        for &b in &self.bytes[..self.pos.min(self.bytes.len())] {
            if b == b'\n' {
                line += 1;
                col = 1;
            } else {
                col += 1;
            }
        }
        (line, col)
    }

    fn err(&self, msg: impl Into<String>) -> String {
        let (l, c) = self.line_col();
        format!("{} at line {l}, col {c}", msg.into())
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn consume(&mut self, expected: u8) -> Result<(), String> {
        self.skip_whitespace();
        if self.peek() == Some(expected) {
            self.pos += 1;
            Ok(())
        } else {
            let found = self.peek().map(|b| (b as char).to_string()).unwrap_or_else(|| "EOF".into());
            Err(self.err(format!("expected '{}', found '{found}'", expected as char)))
        }
    }

    fn parse_value(&mut self) -> Result<JsonValue, String> {
        self.skip_whitespace();
        if self.depth > 64 {
            return Err(self.err("JSON recursion depth limit exceeded"));
        }
        let b = self.peek().ok_or_else(|| self.err("unexpected EOF"))?;
        match b {
            b'{' => self.parse_object(),
            b'[' => self.parse_array(),
            b'"' => self.parse_string().map(JsonValue::String),
            b't' | b'f' => self.parse_bool().map(JsonValue::Bool),
            b'n' => self.parse_null().map(|_| JsonValue::Null),
            b'-' | b'0'..=b'9' => self.parse_number().map(JsonValue::Number),
            _ => Err(self.err(format!("unexpected character '{}'", b as char))),
        }
    }

    fn parse_string(&mut self) -> Result<String, String> {
        self.consume(b'"')?;
        let mut res = String::new();
        while self.pos < self.bytes.len() {
            let b = self.bytes[self.pos];
            self.pos += 1;
            match b {
                b'"' => return Ok(res),
                b'\\' => {
                    let esc = self.bytes.get(self.pos).ok_or_else(|| self.err("unexpected EOF in escape"))?;
                    self.pos += 1;
                    match esc {
                        b'"' => res.push('"'),
                        b'\\' => res.push('\\'),
                        b'/' => res.push('/'),
                        b'b' => res.push('\x08'),
                        b'f' => res.push('\x0c'),
                        b'n' => res.push('\n'),
                        b'r' => res.push('\r'),
                        b't' => res.push('\t'),
                        b'u' => {
                            if self.pos + 4 > self.bytes.len() {
                                return Err(self.err("truncated \\u unicode escape"));
                            }
                            let hex = &self.src[self.pos..self.pos + 4];
                            self.pos += 4;
                            let cp = u16::from_str_radix(hex, 16).map_err(|_| self.err("invalid hex in \\u escape"))?;
                            let c = char::from_u32(cp as u32).unwrap_or('\u{FFFD}');
                            res.push(c);
                        }
                        _ => return Err(self.err(format!("invalid escape '\\{}'", *esc as char))),
                    }
                }
                c if c < 0x20 => {
                    return Err(self.err("unescaped control character in string"));
                }
                _ => {
                    // Collect UTF-8 character starting here
                    self.pos -= 1;
                    let remaining = &self.src[self.pos..];
                    let ch = remaining.chars().next().ok_or_else(|| self.err("invalid UTF-8 in string"))?;
                    self.pos += ch.len_utf8();
                    res.push(ch);
                }
            }
        }
        Err(self.err("unclosed string literal"))
    }

    fn parse_number(&mut self) -> Result<f64, String> {
        let start = self.pos;
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        if self.pos >= self.bytes.len() {
            return Err(self.err("unexpected EOF in number"));
        }
        while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
            self.pos += 1;
        }
        if self.peek() == Some(b'.') {
            self.pos += 1;
            let frac_start = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if self.pos == frac_start {
                return Err(self.err("expected digits after decimal point"));
            }
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_start = self.pos;
            while self.pos < self.bytes.len() && self.bytes[self.pos].is_ascii_digit() {
                self.pos += 1;
            }
            if self.pos == exp_start {
                return Err(self.err("expected digits in exponent"));
            }
        }
        let num_str = &self.src[start..self.pos];
        num_str.parse::<f64>().map_err(|_| self.err(format!("invalid number '{num_str}'")))
    }

    fn parse_bool(&mut self) -> Result<bool, String> {
        if self.src[self.pos..].starts_with("true") {
            self.pos += 4;
            Ok(true)
        } else if self.src[self.pos..].starts_with("false") {
            self.pos += 5;
            Ok(false)
        } else {
            Err(self.err("invalid boolean"))
        }
    }

    fn parse_null(&mut self) -> Result<(), String> {
        if self.src[self.pos..].starts_with("null") {
            self.pos += 4;
            Ok(())
        } else {
            Err(self.err("invalid null literal"))
        }
    }

    fn parse_array(&mut self) -> Result<JsonValue, String> {
        self.consume(b'[')?;
        self.depth += 1;
        self.skip_whitespace();
        let mut elements = Vec::new();
        if self.peek() == Some(b']') {
            self.pos += 1;
            self.depth -= 1;
            return Ok(JsonValue::Array(elements));
        }
        loop {
            let elem = self.parse_value()?;
            elements.push(elem);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                    self.skip_whitespace();
                    if self.peek() == Some(b']') {
                        return Err(self.err("trailing comma in array"));
                    }
                }
                Some(b']') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("expected ',' or ']' in array")),
            }
        }
        self.depth -= 1;
        Ok(JsonValue::Array(elements))
    }

    fn parse_object(&mut self) -> Result<JsonValue, String> {
        self.consume(b'{')?;
        self.depth += 1;
        self.skip_whitespace();
        let mut fields = Vec::new();
        if self.peek() == Some(b'}') {
            self.pos += 1;
            self.depth -= 1;
            return Ok(JsonValue::Object(fields));
        }
        loop {
            self.skip_whitespace();
            let key = self.parse_string()?;
            self.consume(b':')?;
            let val = self.parse_value()?;
            fields.push((key, val));
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => {
                    self.pos += 1;
                    self.skip_whitespace();
                    if self.peek() == Some(b'}') {
                        return Err(self.err("trailing comma in object"));
                    }
                }
                Some(b'}') => {
                    self.pos += 1;
                    break;
                }
                _ => return Err(self.err("expected ',' or '}' in object")),
            }
        }
        self.depth -= 1;
        Ok(JsonValue::Object(fields))
    }
}

fn fmt_f32(val: f32) -> String {
    if val.is_nan() || val.is_infinite() {
        return "0.0".to_string();
    }
    let s = format!("{val}");
    if !s.contains('.') && !s.contains('e') && !s.contains('E') {
        format!("{s}.0")
    } else {
        s
    }
}

fn escape_json_str(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}

// -----------------------------------------------------------------------------
// Built-in scripts: 5 sampled paths + 6 motion homages
// -----------------------------------------------------------------------------

/// Samples `CameraMode::Orbit` into editable keyframes.
pub fn orbit_script() -> CameraScript {
    // Default dials: speed 1.25, distance 0.4, focus 0.76, height 0.78, zoom 0.39
    // Full lap: 360 / 1.25 = 288 frames.
    // In model.rs, effective elevation is height * distance = 0.78 * 0.4 = 0.312
    let frames_total = 288.0f32;
    let step_count = 8;
    let mut keys = Vec::with_capacity(step_count + 1);
    for i in 0..=step_count {
        let frac = i as f32 / step_count as f32;
        let frame = frac * frames_total;
        let azimuth = frac * 360.0;
        keys.push(Key::new(frame, azimuth, 0.312, 0.4, 0.0, 0.39, 0.0));
    }
    CameraScript::new("orbit", keys, true, Easing::Linear)
}

/// Samples `CameraMode::Tornado` into editable keyframes.
pub fn tornado_script() -> CameraScript {
    // Default dials: speed 0.55, distance 2.25, focus 0.64, height 0.68, zoom 0.285
    use std::f32::consts::TAU;
    let speed = 0.55f32;
    let height = 0.68f32;
    let dist = 2.25f32;
    let lap_frames = 360.0 / speed;
    let step_count = 16;
    let mut keys = Vec::with_capacity(step_count + 1);
    for i in 0..=step_count {
        let frame = (i as f32 / step_count as f32) * lap_frames;
        let a = (speed * frame).to_radians();
        let lap = (a / TAU).rem_euclid(1.0);
        let eased = (lap * TAU).cos() * -0.5 + 0.5;
        let elev = (1.0 - eased) * height.max(0.3) * 2.0 * dist;
        let d = dist * (1.0 - 0.85 * eased).max(0.1);
        let azimuth = (3.0 * a).to_degrees();
        keys.push(Key::new(frame, azimuth, elev, d, 0.0, 0.285, 0.0));
    }
    CameraScript::new("tornado", keys, true, Easing::Linear)
}

/// Samples `CameraMode::Crane` into editable keyframes.
pub fn crane_script() -> CameraScript {
    // Default dials: speed 1.25, distance 0.4, focus 0.76, height 0.78, zoom 0.39
    use std::f32::consts::TAU;
    let speed = 1.25f32;
    let dist = 0.4f32;
    let lap_frames = 360.0 / speed;
    let step_count = 16;
    let mut keys = Vec::with_capacity(step_count + 1);
    for i in 0..=step_count {
        let frame = (i as f32 / step_count as f32) * lap_frames;
        let a = (speed * frame).to_radians();
        let lap = (a / TAU).rem_euclid(1.0);
        let eased = (lap * TAU).cos() * -0.5 + 0.5;
        let elev = (0.05 + 0.95 * eased) * dist;
        let d = dist * (1.0 - 0.95 * eased);
        keys.push(Key::new(frame, 0.0, elev, d, 0.0, 0.39, 0.0));
    }
    CameraScript::new("crane", keys, true, Easing::Linear)
}

/// Samples `CameraMode::FigureEight` into editable keyframes.
pub fn figure_eight_script() -> CameraScript {
    // Level lemniscate at height 0.78, distance 0.4
    let speed = 1.25f32;
    let dist = 0.4f32;
    let elev = 0.78 * dist;
    let lap_frames = 360.0 / speed;
    let step_count = 16;
    let mut keys = Vec::with_capacity(step_count + 1);
    for i in 0..=step_count {
        let frame = (i as f32 / step_count as f32) * lap_frames;
        let a = (speed * frame).to_radians();
        let fx = a.sin() * dist;
        let fy = 0.5 * (2.0 * a).sin() * dist;
        let d = (fx * fx + fy * fy).sqrt().max(0.001);
        let azimuth = fy.atan2(fx).to_degrees();
        keys.push(Key::new(frame, azimuth, elev, d, 0.0, 0.39, 0.0));
    }
    CameraScript::new("figure-eight", keys, true, Easing::Linear)
}

/// Samples `CameraMode::Zenith` into editable keyframes.
pub fn zenith_script() -> CameraScript {
    // Overhead vantage looking straight down
    let keys = vec![
        Key::new(0.0, 0.0, 0.4, 0.001, 0.0, 0.39, 0.0),
        Key::new(300.0, 0.0, 0.4, 0.001, 0.0, 0.39, 0.0),
    ];
    CameraScript::new("zenith", keys, true, Easing::Linear)
}

/// Borrows the gradual deceleration into a resting frame; avoids console wordmarks and boot chimes.
pub fn settle_on_the_release() -> CameraScript {
    let keys = vec![
        Key::new(0.0, 0.0, 0.6, 0.8, 0.2, 0.45, 0.0),
        Key::new(80.0, 120.0, 0.5, 0.65, 0.1, 0.42, 0.0),
        Key::new(160.0, 200.0, 0.4, 0.55, 0.05, 0.4, 0.0),
        Key::new(240.0, 240.0, 0.35, 0.5, 0.0, 0.39, 0.0).with_easing(Easing::EaseInOut),
        Key::new(300.0, 245.0, 0.35, 0.5, 0.0, 0.39, 0.0),
    ];
    CameraScript::new("settle-on-the-release", keys, false, Easing::EaseInOut)
}

/// Borrows the rising orbital path climbing above the field; avoids ribbon and sphere geometry.
pub fn orbit_and_rise() -> CameraScript {
    let keys = vec![
        Key::new(0.0, 0.0, 0.1, 0.35, 0.5, 0.35, 0.0),
        Key::new(75.0, 135.0, 0.35, 0.45, 0.4, 0.38, 0.0),
        Key::new(150.0, 270.0, 0.65, 0.6, 0.25, 0.4, 0.0),
        Key::new(225.0, 405.0, 0.95, 0.75, 0.1, 0.42, 0.0),
        Key::new(300.0, 540.0, 1.15, 0.85, 0.0, 0.45, 0.0),
    ];
    CameraScript::new("orbit-and-rise", keys, false, Easing::EaseInOut)
}

/// Borrows the tightening spiral dive toward the floor; avoids tunnel meshes and particle trails.
pub fn spiral_descent() -> CameraScript {
    let keys = vec![
        Key::new(0.0, 0.0, 1.2, 1.2, 0.0, 0.45, 0.0),
        Key::new(60.0, 180.0, 0.85, 0.85, 0.05, 0.42, 0.0),
        Key::new(120.0, 360.0, 0.55, 0.55, 0.1, 0.38, 0.0),
        Key::new(180.0, 540.0, 0.3, 0.35, 0.15, 0.35, 0.0),
        Key::new(240.0, 720.0, 0.1, 0.2, 0.2, 0.32, 0.0),
    ];
    CameraScript::new("spiral-descent", keys, false, Easing::EaseInOut)
}

/// Borrows the held anticipation pause before snap rotation; avoids rolling cube shapes.
pub fn hold_the_anticipation() -> CameraScript {
    let keys = vec![
        Key::new(0.0, 0.0, 0.4, 0.5, 0.2, 0.39, 0.0).with_easing(Easing::Hold),
        Key::new(60.0, 0.0, 0.4, 0.5, 0.2, 0.39, 0.0).with_easing(Easing::EaseInOut),
        Key::new(80.0, 90.0, 0.4, 0.5, 0.2, 0.39, 0.0).with_easing(Easing::Hold),
        Key::new(140.0, 90.0, 0.4, 0.5, 0.2, 0.39, 0.0).with_easing(Easing::EaseInOut),
        Key::new(160.0, 180.0, 0.4, 0.5, 0.2, 0.39, 0.0).with_easing(Easing::Hold),
        Key::new(220.0, 180.0, 0.4, 0.5, 0.2, 0.39, 0.0).with_easing(Easing::EaseInOut),
        Key::new(240.0, 270.0, 0.4, 0.5, 0.2, 0.39, 0.0).with_easing(Easing::Hold),
        Key::new(300.0, 270.0, 0.4, 0.5, 0.2, 0.39, 0.0),
    ];
    CameraScript::new("hold-the-anticipation", keys, true, Easing::Linear)
}

/// Borrows the low horizontal dolly tilting upward into the towers; avoids wave overlay graphics.
pub fn push_in_and_tilt() -> CameraScript {
    let keys = vec![
        Key::new(0.0, 0.0, 0.05, 0.9, 0.0, 0.35, 0.0),
        Key::new(90.0, 15.0, 0.08, 0.55, 0.3, 0.38, 0.0),
        Key::new(180.0, 30.0, 0.12, 0.3, 0.65, 0.42, 0.0),
        Key::new(270.0, 45.0, 0.15, 0.18, 0.95, 0.46, 0.0),
    ];
    CameraScript::new("push-in-and-tilt", keys, false, Easing::EaseInOut)
}

/// Borrows the rapid pull-back from intimate focus to high overview; avoids channel grid artwork.
pub fn pull_back_reveal() -> CameraScript {
    let keys = vec![
        Key::new(0.0, 45.0, 0.05, 0.15, 0.8, 0.3, 0.0),
        Key::new(60.0, 40.0, 0.2, 0.3, 0.5, 0.35, 0.0),
        Key::new(140.0, 30.0, 0.6, 0.7, 0.2, 0.4, 0.0),
        Key::new(220.0, 15.0, 1.0, 1.2, 0.0, 0.45, 0.0),
        Key::new(300.0, 0.0, 1.25, 1.4, -0.1, 0.48, 0.0),
    ];
    CameraScript::new("pull-back-reveal", keys, false, Easing::EaseInOut)
}

/// All built-in scripts: the five sampled paths and six motion homages.
pub fn built_in_scripts() -> Vec<CameraScript> {
    vec![
        orbit_script(),
        tornado_script(),
        crane_script(),
        figure_eight_script(),
        zenith_script(),
        settle_on_the_release(),
        orbit_and_rise(),
        spiral_descent(),
        hold_the_anticipation(),
        push_in_and_tilt(),
        pull_back_reveal(),
    ]
}

// -----------------------------------------------------------------------------
// Deterministic montage shuffle across built-ins
// -----------------------------------------------------------------------------

struct Prng(u64);

impl Prng {
    fn new(seed: u64) -> Self {
        Self(if seed == 0 { 0x853c49e6748fea9b } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }

    fn next_range(&mut self, max: usize) -> usize {
        (self.next_u64() as usize) % max.max(1)
    }
}

/// Creates a deterministic montage across built-in paths with eased transitions.
pub fn shuffle(seed: u64, frames_per_shot: u32) -> CameraScript {
    let mut rng = Prng::new(seed);
    let pool = built_in_scripts();
    let shot_len = (frames_per_shot.max(30)) as f32;
    let shot_count = 5;

    let mut out_keys = Vec::new();
    let mut last_idx = usize::MAX;

    for shot in 0..shot_count {
        let mut idx = rng.next_range(pool.len());
        if idx == last_idx {
            idx = (idx + 1) % pool.len();
        }
        last_idx = idx;

        let base = &pool[idx];
        let base_start = base.keys.first().map(|k| k.frame).unwrap_or(0.0);
        let base_end = base.keys.last().map(|k| k.frame).unwrap_or(shot_len);
        let base_span = (base_end - base_start).max(1.0);

        let shot_time_start = shot as f32 * shot_len;

        for k in &base.keys {
            let frac = (k.frame - base_start) / base_span;
            let shot_frame = shot_time_start + frac * shot_len;

            // Ease the transition out into the next shot
            let easing = if frac >= 0.85 {
                Some(Easing::EaseInOut)
            } else {
                k.easing
            };

            out_keys.push(Key {
                frame: shot_frame,
                azimuth_deg: k.azimuth_deg,
                elevation: k.elevation,
                distance: k.distance,
                aim: k.aim,
                lens: k.lens,
                roll_deg: k.roll_deg,
                easing,
            });
        }
    }

    CameraScript::new(format!("shuffle-{seed}"), out_keys, true, Easing::Linear)
}

// -----------------------------------------------------------------------------
// Unit tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::CameraMode;

    #[test]
    fn test_json_round_trip_builtins() {
        for script in built_in_scripts() {
            let json = script.to_json();
            let parsed = CameraScript::from_json(&json)
                .unwrap_or_else(|e| panic!("failed to parse {}: {e}", script.name));
            assert_eq!(
                script, parsed,
                "round-trip mismatch for script '{}'",
                script.name
            );
        }
    }

    #[test]
    fn test_malformed_json_fuzz() {
        let malformed_samples = [
            "",
            "   ",
            "{",
            "}",
            "{ name: 123 }",
            "{ \"name\": 123 }",
            "{ \"name\": \"test\" }", // missing keys
            "{ \"name\": \"test\", \"keys\": \"not an array\" }",
            "{ \"name\": \"test\", \"keys\": [123] }",
            "{ \"name\": \"test\", \"keys\": [{ \"frame\": \"abc\" }] }",
            "{ \"name\": \"test\", \"keys\": [{ \"frame\": 0 }] }", // missing other fields
            "{ \"name\": \"test\", \"keys\": [{ \"frame\": 0, \"azimuth_deg\": 0, \"elevation\": 0, \"distance\": 1, \"aim\": 0, \"lens\": 1, \"roll_deg\": 0 }], \"extra\": ",
            "\"unclosed string",
            "[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]]",
            "{\"name\": \"test\", \"keys\": [], \"trailing\": 123} trailing_garbage",
            "{\"name\": \"test\", \"keys\": [,] }",
            "{\"name\": \"test\", \"keys\": [], }",
            "--123",
            "1.2.3.4",
            "{\"name\": \"test\x00\"}",
        ];

        for sample in &malformed_samples {
            assert!(
                CameraScript::from_json(sample).is_err(),
                "malformed sample should fail: {sample:?}"
            );
        }

        // Mutation / fuzz testing: truncated slices of a valid JSON
        let valid_json = orbit_script().to_json();
        for len in 0..valid_json.len() - 1 {
            let truncated = &valid_json[..len];
            let _ = CameraScript::from_json(truncated); // Must not panic
        }

        // Pseudo-random garbage strings
        let mut seed = 0x12345678u64;
        for _ in 0..200 {
            let mut garbage = Vec::new();
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let len = (seed % 120) as usize;
            for _ in 0..len {
                seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
                garbage.push((seed & 0xFF) as u8);
            }
            if let Ok(s) = std::str::from_utf8(&garbage) {
                let _ = CameraScript::from_json(s); // Must not panic
            }
        }
    }

    #[test]
    fn test_orbit_equivalence() {
        let script = orbit_script();
        let speed = 1.25f32;
        let height = 0.78f32;
        let distance = 0.4f32;
        let zoom = 0.39f32;

        let centre = Vec3::new(12.0, -8.0, 160.0);
        let radius = 184.0f32;
        let vertical = Vec3::Z;
        let front = -Vec3::Y;
        let video = VideoMode::Ntsc;

        let orbit_radius = radius * distance;

        for frame in [0.0, 10.0, 36.0, 72.0, 100.0, 144.0, 180.0, 216.0, 250.0, 288.0] {
            let orbit_cam = CameraMode::Orbit
                .view(frame, speed, centre, orbit_radius, height, vertical, front, video)
                .expect("orbit view");
            let script_cam = script
                .view(frame, centre, radius, vertical, front, video)
                .expect("script view");

            let pos_diff = (script_cam.position - orbit_cam.position).length();
            assert!(
                pos_diff < 0.05,
                "position mismatch at frame {frame}: diff {pos_diff}, script={:?}, orbit={:?}",
                script_cam.position,
                orbit_cam.position
            );

            let fwd_dot = script_cam.forward.dot(orbit_cam.forward);
            assert!(
                fwd_dot > 0.999,
                "forward mismatch at frame {frame}: dot {fwd_dot}"
            );

            let up_dot = script_cam.up.dot(orbit_cam.up);
            assert!(
                up_dot > 0.999,
                "up mismatch at frame {frame}: dot {up_dot}"
            );

            assert!(
                (script_cam.zoom - zoom).abs() < 1e-4,
                "zoom mismatch at frame {frame}"
            );
        }
    }

    #[test]
    fn test_shuffle_determinism() {
        let s1 = shuffle(42, 120);
        let s2 = shuffle(42, 120);
        assert_eq!(s1, s2, "shuffle must be deterministic for identical seed");

        let s3 = shuffle(999, 120);
        assert_ne!(s1, s3, "shuffle should vary with different seeds");

        // Verify JSON serialization of shuffled montage
        let json = s1.to_json();
        let parsed = CameraScript::from_json(&json).expect("parse shuffled json");
        assert_eq!(s1, parsed);
    }

    #[test]
    fn test_easing_curves() {
        assert_eq!(Easing::Linear.ease(0.0), 0.0);
        assert_eq!(Easing::Linear.ease(0.5), 0.5);
        assert_eq!(Easing::Linear.ease(1.0), 1.0);

        assert!((Easing::EaseInOut.ease(0.0) - 0.0).abs() < 1e-6);
        assert!((Easing::EaseInOut.ease(0.5) - 0.5).abs() < 1e-6);
        assert!((Easing::EaseInOut.ease(1.0) - 1.0).abs() < 1e-6);
        // Midpoint slope is steeper than linear
        assert!(Easing::EaseInOut.ease(0.25) < 0.25);
        assert!(Easing::EaseInOut.ease(0.75) > 0.75);

        assert_eq!(Easing::Hold.ease(0.0), 0.0);
        assert_eq!(Easing::Hold.ease(0.99), 0.0);
        assert_eq!(Easing::Hold.ease(1.0), 1.0);
    }
}
