//! Number validation for the Settings editor's numeric inputs.
//!
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsTreeModels.ts @ 50e1350fc78d

/// Parses and validates `text` for a number setting. `Err` carries the message
/// shown under the input; the setting is not written while it is invalid.
pub fn validate_number(text: &str, min: Option<f64>, max: Option<f64>, integer: bool) -> Result<f64, String> {
    let t = text.trim();
    let n: f64 = t.parse().map_err(|_| "Value must be a number.".to_string())?;
    if !n.is_finite() {
        return Err("Value must be a number.".into());
    }
    if integer && n.fract() != 0.0 {
        return Err("Value must be an integer.".into());
    }
    if let Some(min) = min.filter(|m| n < *m) {
        return Err(format!("Value must be greater than or equal to {}.", fmt(min)));
    }
    if let Some(max) = max.filter(|m| n > *m) {
        return Err(format!("Value must be less than or equal to {}.", fmt(max)));
    }
    Ok(n)
}

fn fmt(n: f64) -> String {
    if n.fract() == 0.0 { format!("{}", n as i64) } else { n.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_numbers() {
        assert_eq!(validate_number("14", Some(6.0), Some(100.0), true), Ok(14.0));
        assert_eq!(validate_number(" 1.5 ", None, None, false), Ok(1.5));
    }

    #[test]
    fn rejects_non_numbers() {
        assert!(validate_number("", None, None, false).is_err());
        assert!(validate_number("abc", None, None, false).is_err());
        assert!(validate_number("NaN", None, None, false).is_err());
    }

    #[test]
    fn integer_constraint() {
        assert_eq!(validate_number("1.5", None, None, true), Err("Value must be an integer.".into()));
    }

    #[test]
    fn range_constraints() {
        assert_eq!(validate_number("3", Some(6.0), None, true), Err("Value must be greater than or equal to 6.".into()));
        assert_eq!(validate_number("300", None, Some(100.0), true), Err("Value must be less than or equal to 100.".into()));
        assert!(validate_number("6", Some(6.0), Some(100.0), true).is_ok());
    }
}
