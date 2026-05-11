//! `Hotkey` 値オブジェクト — グローバルホットキーのコンボ文字列。

use std::fmt;
use std::hash::{Hash, Hasher};

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

// -------------------------------------------------------------------
// HotkeyParseError
// -------------------------------------------------------------------

/// `Hotkey::parse` が返すエラー。
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HotkeyParseError {
    /// 空文字列。
    #[error("hotkey string is empty")]
    Empty,

    /// 修飾キーが含まれていない（最低 1 個必要）。
    #[error("hotkey must include at least one modifier (ctrl, alt, shift, meta)")]
    NoModifier,

    /// 主キーが無効（英数字 1 文字または f1〜f12 以外）。
    #[error("invalid key: '{raw}'. expected a-z, 0-9, or f1-f12")]
    InvalidKey {
        /// 不正なキー文字列。
        raw: String,
    },

    /// `+` 区切りパーツが多すぎる（最大 5）。
    #[error("too many '+'-separated parts (max 5)")]
    TooManyParts,
}

// -------------------------------------------------------------------
// Hotkey
// -------------------------------------------------------------------

/// グローバルホットキーのコンボを表す値オブジェクト。
///
/// 内部状態は正規化済み文字列のみ（`"alt+ctrl+1"` 形式）。
/// modifiers / key の個別フィールドを廃止し、DRY / Tell Don't Ask を徹底する。
#[derive(Clone)]
pub struct Hotkey {
    normalized: Box<str>,
}

impl Hotkey {
    /// 文字列をパースして正規化した `Hotkey` を構築する。
    ///
    /// 正規化: 修飾キーをアルファベット順（alt → ctrl → meta → shift）+ 主キーで並べ替え。
    ///
    /// # Errors
    /// 不正な形式の場合 `HotkeyParseError` を返す。
    pub fn parse(s: &str) -> Result<Self, HotkeyParseError> {
        if s.is_empty() {
            return Err(HotkeyParseError::Empty);
        }

        let parts: Vec<&str> = s.split('+').collect();
        if parts.len() > 5 {
            return Err(HotkeyParseError::TooManyParts);
        }

        let mut has_alt = false;
        let mut has_ctrl = false;
        let mut has_meta = false;
        let mut has_shift = false;
        let mut main_key: Option<String> = None;

        for part in &parts {
            let lower = part.to_lowercase();
            match lower.as_str() {
                "alt" => has_alt = true,
                "ctrl" | "control" => has_ctrl = true,
                "meta" | "super" | "cmd" | "win" => has_meta = true,
                "shift" => has_shift = true,
                other => {
                    // 主キー候補
                    let valid = Self::is_valid_key(other);
                    if valid {
                        if main_key.is_some() {
                            // 主キーが複数 → 2 個目を不正扱い
                            return Err(HotkeyParseError::InvalidKey {
                                raw: other.to_owned(),
                            });
                        }
                        main_key = Some(lower.clone());
                    } else {
                        return Err(HotkeyParseError::InvalidKey {
                            raw: other.to_owned(),
                        });
                    }
                }
            }
        }

        // 修飾キーが 0 個
        if !has_alt && !has_ctrl && !has_meta && !has_shift {
            return Err(HotkeyParseError::NoModifier);
        }

        // 主キーが 0 個
        let key = main_key.ok_or_else(|| HotkeyParseError::InvalidKey { raw: String::new() })?;

        // 正規化: alt → ctrl → meta → shift + 主キー（アルファベット順）
        let mut normalized = String::new();
        if has_alt {
            normalized.push_str("alt+");
        }
        if has_ctrl {
            normalized.push_str("ctrl+");
        }
        if has_meta {
            normalized.push_str("meta+");
        }
        if has_shift {
            normalized.push_str("shift+");
        }
        normalized.push_str(&key);

        Ok(Self {
            normalized: normalized.into_boxed_str(),
        })
    }

    /// 正規化済みホットキー文字列を返す。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.normalized
    }

    /// 与えられた文字列が主キーとして有効かどうかを判定する（ASCII英数字 or f1〜f12）。
    fn is_valid_key(s: &str) -> bool {
        // 単一 ASCII 英数字
        if s.len() == 1 {
            let c = s.chars().next().unwrap();
            return c.is_ascii_alphanumeric();
        }
        // f1〜f12
        if let Some(rest) = s.strip_prefix('f') {
            if let Ok(n) = rest.parse::<u8>() {
                return (1..=12).contains(&n);
            }
        }
        false
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.normalized)
    }
}

impl fmt::Debug for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hotkey({:?})", self.normalized)
    }
}

impl PartialEq for Hotkey {
    fn eq(&self, other: &Self) -> bool {
        self.normalized == other.normalized
    }
}

impl Eq for Hotkey {}

impl Hash for Hotkey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.normalized.hash(state);
    }
}

impl Serialize for Hotkey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.normalized)
    }
}

impl<'de> Deserialize<'de> for Hotkey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ctrl_alt_1_normalizes() {
        let h = Hotkey::parse("ctrl+alt+1").unwrap();
        assert_eq!(h.as_str(), "alt+ctrl+1");
    }

    #[test]
    fn test_parse_uppercase_normalizes() {
        let h = Hotkey::parse("Ctrl+Alt+1").unwrap();
        assert_eq!(h.as_str(), "alt+ctrl+1");
    }

    #[test]
    fn test_parse_same_combo_is_equal() {
        let h1 = Hotkey::parse("ctrl+alt+1").unwrap();
        let h2 = Hotkey::parse("alt+ctrl+1").unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_parse_empty_returns_error() {
        assert!(matches!(Hotkey::parse(""), Err(HotkeyParseError::Empty)));
    }

    #[test]
    fn test_parse_no_modifier_returns_error() {
        assert!(matches!(
            Hotkey::parse("a"),
            Err(HotkeyParseError::NoModifier)
        ));
    }

    #[test]
    fn test_parse_invalid_key_returns_error() {
        let err = Hotkey::parse("ctrl+!").unwrap_err();
        assert!(matches!(err, HotkeyParseError::InvalidKey { .. }));
    }

    #[test]
    fn test_parse_f12_is_valid() {
        let h = Hotkey::parse("ctrl+f12").unwrap();
        assert_eq!(h.as_str(), "ctrl+f12");
    }

    #[test]
    fn test_parse_too_many_parts_returns_error() {
        assert!(matches!(
            Hotkey::parse("ctrl+alt+shift+meta+a+b"),
            Err(HotkeyParseError::TooManyParts)
        ));
    }

    #[test]
    fn test_display_returns_normalized() {
        let h = Hotkey::parse("ctrl+alt+a").unwrap();
        assert_eq!(h.to_string(), "alt+ctrl+a");
    }
}
