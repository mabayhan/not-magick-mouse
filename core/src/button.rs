use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Button {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl Button {
    pub const ALL: [Button; 5] = [Button::Left, Button::Right, Button::Middle, Button::X1, Button::X2];

    pub fn name(self) -> &'static str {
        match self {
            Button::Left => "left",
            Button::Right => "right",
            Button::Middle => "middle",
            Button::X1 => "x1",
            Button::X2 => "x2",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Button::Left => "Left",
            Button::Right => "Right",
            Button::Middle => "Middle",
            Button::X1 => "Back (X1)",
            Button::X2 => "Forward (X2)",
        }
    }
}

impl fmt::Display for Button {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl FromStr for Button {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_ascii_lowercase().as_str() {
            "left" => Ok(Button::Left),
            "right" => Ok(Button::Right),
            "middle" => Ok(Button::Middle),
            "x1" | "back" => Ok(Button::X1),
            "x2" | "forward" => Ok(Button::X2),
            other => Err(format!("unknown mouse button '{other}'")),
        }
    }
}
