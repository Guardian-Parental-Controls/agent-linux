use std::collections::HashMap;
use std::convert::TryFrom;

pub type AllowedHoursDay = HashMap<String, HashMap<String, i32>>;

pub fn build_full_access_day() -> AllowedHoursDay {
    let mut day_map = AllowedHoursDay::new();
    for hour in 0..24 {
        day_map.insert(
            hour.to_string(),
            HashMap::from([
                ("STARTMIN".to_string(), 0),
                ("ENDMIN".to_string(), 60),
                ("UACC".to_string(), 0),
            ]),
        );
    }
    day_map
}

pub fn parse_day_hours(
    value: &serde_json::Value,
    day_str: &str,
) -> Result<AllowedHoursDay, String> {
    let day_object = value
        .as_object()
        .ok_or_else(|| format!("Allowed-hours payload for day {day_str} must be an object"))?;

    let mut parsed = AllowedHoursDay::new();
    for (hour_key, spec_value) in day_object {
        let spec_object = spec_value.as_object().ok_or_else(|| {
            format!("Allowed-hours spec for day {day_str}, hour {hour_key} must be an object")
        })?;

        let mut spec = HashMap::new();
        for field in ["STARTMIN", "ENDMIN", "UACC"] {
            let raw = spec_object
                .get(field)
                .and_then(|value| value.as_i64())
                .ok_or_else(|| {
                    format!(
                        "Allowed-hours spec for day {day_str}, hour {hour_key} is missing integer field {field}"
                    )
                })?;
            let parsed_value = i32::try_from(raw).map_err(|_| {
                format!(
                    "Allowed-hours spec for day {day_str}, hour {hour_key} has out-of-range field {field}"
                )
            })?;
            spec.insert(field.to_string(), parsed_value);
        }

        parsed.insert(hour_key.clone(), spec);
    }

    Ok(parsed)
}

pub fn schedule_to_day_limits(
    schedule: &serde_json::Map<String, serde_json::Value>,
) -> Result<(Vec<String>, Vec<i32>), String> {
    let day_order = [
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
    ];

    let mut allowed_days = Vec::new();
    let mut day_limits = Vec::new();

    for (index, day_name) in day_order.iter().enumerate() {
        let hours = schedule
            .get(*day_name)
            .and_then(|value| value.as_f64())
            .unwrap_or(0.0);

        if hours.is_sign_negative() {
            return Err(format!(
                "Schedule value for {day_name} must not be negative"
            ));
        }

        if hours > 0.0 {
            allowed_days.push((index + 1).to_string());
        }

        let seconds = (hours * 3600.0).round();
        if !(0.0..=(24.0 * 3600.0)).contains(&seconds) {
            return Err(format!("Schedule value for {day_name} is out of range"));
        }
        day_limits.push(seconds as i32);
    }

    Ok((allowed_days, day_limits))
}

pub fn is_valid_linux_username(username: &str) -> bool {
    if username.is_empty() || username.len() > 32 {
        return false;
    }
    let mut chars = username.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !matches!(first, 'a'..='z' | '_') {
        return false;
    }
    for ch in chars {
        if !(ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '_' | '-')) {
            return false;
        }
    }
    true
}

pub fn command_requires_linux_username(action: &str) -> bool {
    !matches!(
        action,
        "get_domain_policy_state"
            | "begin_domain_policy_sync"
            | "delete_domain_policy_sources"
            | "sync_domain_policy_chunk"
            | "update_domain_policy_manifest"
            | "finalize_domain_policy_sync"
            | "abort_domain_policy_sync"
            | "sync_domain_policy"
            | "sync_screenshot_policy"
            | "capture_screenshot"
            | "unenroll"
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_day_hours, schedule_to_day_limits};

    #[test]
    fn schedule_conversion_preserves_all_days() {
        let schedule = serde_json::json!({
            "monday": 2.0,
            "tuesday": 0.0,
            "wednesday": 1.5,
            "thursday": 0.0,
            "friday": 0.0,
            "saturday": 0.0,
            "sunday": 0.25
        });

        let (allowed_days, day_limits) =
            schedule_to_day_limits(schedule.as_object().unwrap()).unwrap();
        assert_eq!(allowed_days, vec!["1", "3", "7"]);
        assert_eq!(day_limits, vec![7200, 0, 5400, 0, 0, 0, 900]);
    }

    #[test]
    fn day_hours_parser_requires_expected_integer_fields() {
        let payload = serde_json::json!({
            "9": {"STARTMIN": 30, "ENDMIN": 60, "UACC": 0},
            "10": {"STARTMIN": 0, "ENDMIN": 60, "UACC": 0}
        });

        let parsed = parse_day_hours(&payload, "1").unwrap();
        assert_eq!(parsed["9"]["STARTMIN"], 30);
        assert_eq!(parsed["9"]["ENDMIN"], 60);
        assert_eq!(parsed["10"]["UACC"], 0);
    }
}
