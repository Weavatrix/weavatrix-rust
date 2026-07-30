use blazingly_json::Value;

pub(super) fn similarity_permille(args: &Value) -> Result<u16, String> {
    let Some(raw) = args.get("min_similarity") else {
        return Ok(800);
    };
    let value = raw.as_f64().ok_or_else(|| {
        "min_similarity must be a number: 0..1 as a fraction or 0..100 as a percent".to_owned()
    })?;
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err("min_similarity must be 0..1 as a fraction or 0..100 as a percent".to_owned());
    }
    let fraction = if value <= 1.0 { value } else { value / 100.0 };
    format!("{:.0}", fraction * 1_000.0)
        .parse::<u16>()
        .map_err(|error| format!("could not normalize min_similarity: {error}"))
}

#[cfg(test)]
mod tests {
    use super::similarity_permille;
    use blazingly_json::json;

    #[test]
    fn accepts_equivalent_fraction_and_percent_thresholds() {
        assert_eq!(similarity_permille(&json!({})).unwrap(), 800);
        assert_eq!(
            similarity_permille(&json!({"min_similarity": 0.92})).unwrap(),
            920
        );
        assert_eq!(
            similarity_permille(&json!({"min_similarity": 92})).unwrap(),
            920
        );
        assert_eq!(
            similarity_permille(&json!({"min_similarity": 92.3})).unwrap(),
            923
        );
        assert_eq!(
            similarity_permille(&json!({"min_similarity": 1})).unwrap(),
            1_000
        );
        assert_eq!(
            similarity_permille(&json!({"min_similarity": 100})).unwrap(),
            1_000
        );
    }

    #[test]
    fn rejects_non_numeric_and_out_of_range_thresholds() {
        for invalid in [
            json!({"min_similarity": -0.1}),
            json!({"min_similarity": 100.1}),
            json!({"min_similarity": "92"}),
        ] {
            assert!(similarity_permille(&invalid).is_err(), "{invalid:?}");
        }
    }
}
