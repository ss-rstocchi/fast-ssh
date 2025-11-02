use serde::Deserialize;
use tui::style::Color;

use self::de::deserialize_option_color_hex_string;

macro_rules! def_theme_struct_with_defaults {
    ($($name:ident => $color:expr),+) => {
        #[derive(Debug, Clone, Copy, Deserialize)]
        pub struct Theme {
            $(
                #[serde(deserialize_with = "deserialize_option_color_hex_string")]
                #[serde(default)]
                $name: Option<Color>,
            )+
        }
        impl Theme {
            $(
                #[inline]
                pub fn $name(self) -> Color {
                    self.$name.unwrap_or($color)
                }
            )+
        }
        impl Default for Theme {
            fn default() -> Theme {
                Self {
                    $( $name: Some($color), )+
                }
            }
        }
    };
}

def_theme_struct_with_defaults!(
    text_primary => Color::White,
    text_secondary => Color::Magenta,
    border_color => Color::Magenta
);

fn hex_to_color(hex: &str) -> Option<Color> {
    // Validate format: must be exactly 7 chars and start with #
    if hex.len() != 7 || !hex.starts_with('#') {
        return None;
    }

    // Use safe slicing with get() to avoid panics on non-ASCII
    let r = u8::from_str_radix(hex.get(1..3)?, 16).ok()?;
    let g = u8::from_str_radix(hex.get(3..5)?, 16).ok()?;
    let b = u8::from_str_radix(hex.get(5..7)?, 16).ok()?;

    Some(Color::Rgb(r, g, b))
}

mod de {
    use std::fmt;

    use serde::de::{self, Error, Unexpected, Visitor};

    use super::{hex_to_color, Color};

    pub(crate) fn deserialize_option_color_hex_string<'de, D>(
        deserializer: D,
    ) -> Result<Option<Color>, D::Error>
    where
        D: de::Deserializer<'de>,
    {
        struct ColorVisitor;

        impl Visitor<'_> for ColorVisitor {
            type Value = Option<Color>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a hex string in the format of '#ff0000'")
            }

            fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
            where
                E: Error,
            {
                if let Some(color) = hex_to_color(s) {
                    return Ok(Some(color));
                }

                Err(de::Error::invalid_value(Unexpected::Str(s), &self))
            }
        }

        deserializer.deserialize_any(ColorVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_to_color_valid() {
        assert_eq!(hex_to_color("#ffffff"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(hex_to_color("#000000"), Some(Color::Rgb(0, 0, 0)));
        assert_eq!(hex_to_color("#ff0000"), Some(Color::Rgb(255, 0, 0)));
        assert_eq!(hex_to_color("#00ff00"), Some(Color::Rgb(0, 255, 0)));
        assert_eq!(hex_to_color("#0000ff"), Some(Color::Rgb(0, 0, 255)));
        assert_eq!(hex_to_color("#b967ff"), Some(Color::Rgb(185, 103, 255)));
    }

    #[test]
    fn test_hex_to_color_invalid_length() {
        assert_eq!(hex_to_color("#fff"), None);
        assert_eq!(hex_to_color("#fffffff"), None);
        assert_eq!(hex_to_color(""), None);
        assert_eq!(hex_to_color("#"), None);
    }

    #[test]
    fn test_hex_to_color_invalid_prefix() {
        assert_eq!(hex_to_color("ffffff"), None);
        assert_eq!(hex_to_color("0xffffff"), None);
    }

    #[test]
    fn test_hex_to_color_invalid_chars() {
        assert_eq!(hex_to_color("#gggggg"), None);
        assert_eq!(hex_to_color("#ffgfff"), None);
        assert_eq!(hex_to_color("#zzzzzz"), None);
    }

    #[test]
    fn test_hex_to_color_case_insensitive() {
        assert_eq!(hex_to_color("#FFFFFF"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(hex_to_color("#FfFfFf"), Some(Color::Rgb(255, 255, 255)));
        assert_eq!(hex_to_color("#AbCdEf"), Some(Color::Rgb(171, 205, 239)));
    }

    #[test]
    fn test_theme_default() {
        let theme = Theme::default();
        assert_eq!(theme.text_primary(), Color::White);
        assert_eq!(theme.text_secondary(), Color::Magenta);
        assert_eq!(theme.border_color(), Color::Magenta);
    }

    #[test]
    fn test_theme_custom_colors() {
        let theme = Theme {
            text_primary: Some(Color::Rgb(255, 0, 0)),
            text_secondary: Some(Color::Rgb(0, 255, 0)),
            border_color: Some(Color::Rgb(0, 0, 255)),
        };
        assert_eq!(theme.text_primary(), Color::Rgb(255, 0, 0));
        assert_eq!(theme.text_secondary(), Color::Rgb(0, 255, 0));
        assert_eq!(theme.border_color(), Color::Rgb(0, 0, 255));
    }

    #[test]
    fn test_theme_partial_custom() {
        let theme = Theme {
            text_primary: Some(Color::Rgb(255, 0, 0)),
            text_secondary: None,
            border_color: None,
        };
        assert_eq!(theme.text_primary(), Color::Rgb(255, 0, 0));
        assert_eq!(theme.text_secondary(), Color::Magenta); // Falls back to default
        assert_eq!(theme.border_color(), Color::Magenta); // Falls back to default
    }

    #[test]
    fn test_theme_deserialize_from_yaml() {
        let yaml = "text_primary: \"#ff0000\"\ntext_secondary: \"#00ff00\"\nborder_color: \"#0000ff\"";
        let theme: Theme = serde_yaml::from_str(yaml).expect("Failed to deserialize theme");
        assert_eq!(theme.text_primary(), Color::Rgb(255, 0, 0));
        assert_eq!(theme.text_secondary(), Color::Rgb(0, 255, 0));
        assert_eq!(theme.border_color(), Color::Rgb(0, 0, 255));
    }

    #[test]
    fn test_theme_deserialize_partial_yaml() {
        let yaml = "text_primary: \"#ff0000\"";
        let theme: Theme = serde_yaml::from_str(yaml).expect("Failed to deserialize theme");
        assert_eq!(theme.text_primary(), Color::Rgb(255, 0, 0));
        assert_eq!(theme.text_secondary(), Color::Magenta); // Falls back to default
        assert_eq!(theme.border_color(), Color::Magenta); // Falls back to default
    }
}
