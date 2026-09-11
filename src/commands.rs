use crate::apparmor;
use crate::approval_policy;
use crate::bios_management;
use crate::domain_policy;
use crate::linux_device_policy;
use crate::overlay;
use crate::schedule::{
    build_full_access_day, command_requires_linux_username, is_valid_linux_username,
    parse_day_hours, schedule_to_day_limits,
};
use crate::screenshot;
use crate::screenshots::{get_screenshot_policy_handle, wake_screenshot_scheduler};
use crate::timekpr_dbus::TimekprDbusClient;
use guardian_agent::CommandOutcome;
use guardian_agent::config::clear_agent_enrollment;

fn validate_command_username(action: &str, username: &str) -> Result<(), String> {
    if !command_requires_linux_username(action) {
        return Ok(());
    }
    if !is_valid_linux_username(username) {
        return Err(format!("Invalid Linux username '{username}'"));
    }
    if users::get_user_by_name(username).is_none() {
        return Err(format!(
            "Linux user '{username}' does not exist on this system"
        ));
    }
    Ok(())
}

fn outcome(success: bool, message: String, data: serde_json::Value) -> CommandOutcome {
    CommandOutcome {
        success,
        message,
        data,
    }
}

pub async fn handle_command(
    action: &str,
    username: &str,
    args: &serde_json::Value,
) -> CommandOutcome {
    if let Err(message) = validate_command_username(action, username) {
        return outcome(false, message, serde_json::json!({}));
    }

    match action {
        "get_domain_policy_state" => match domain_policy::get_state_summary().await {
            Ok(data) => outcome(true, "Fetched domain policy state".to_string(), data),
            Err(message) => outcome(false, message, serde_json::json!({})),
        },
        "begin_domain_policy_sync" => match domain_policy::begin_sync_from_args(args).await {
            Ok(message) => outcome(true, message, serde_json::json!({})),
            Err(message) => outcome(false, message, serde_json::json!({})),
        },
        "delete_domain_policy_sources" => match domain_policy::delete_sources_from_args(args).await
        {
            Ok(message) => outcome(true, message, serde_json::json!({})),
            Err(message) => outcome(false, message, serde_json::json!({})),
        },
        "sync_domain_policy_chunk" => {
            match domain_policy::push_source_chunk_from_args(args).await {
                Ok(message) => outcome(true, message, serde_json::json!({})),
                Err(message) => outcome(false, message, serde_json::json!({})),
            }
        }
        "update_domain_policy_manifest" => {
            match domain_policy::update_manifest_from_args(args).await {
                Ok(message) => outcome(true, message, serde_json::json!({})),
                Err(message) => outcome(false, message, serde_json::json!({})),
            }
        }
        "finalize_domain_policy_sync" => match domain_policy::finalize_sync_from_args(args).await {
            Ok(message) => outcome(true, message, serde_json::json!({})),
            Err(message) => outcome(false, message, serde_json::json!({})),
        },
        "abort_domain_policy_sync" => match domain_policy::abort_sync_from_args(args).await {
            Ok(message) => outcome(true, message, serde_json::json!({})),
            Err(message) => outcome(false, message, serde_json::json!({})),
        },
        "sync_domain_policy" => match domain_policy::sync_from_args(args).await {
            Ok(message) => outcome(true, message, serde_json::json!({})),
            Err(message) => outcome(false, message, serde_json::json!({})),
        },
        "validate_user" => {
            let client = match TimekprDbusClient::connect().await {
                Ok(client) => client,
                Err(message) => return outcome(false, message, serde_json::json!({})),
            };
            match client.get_user_information(username).await {
                Ok((result, _message, config)) if result == 0 => outcome(
                    true,
                    "User validated successfully".to_string(),
                    serde_json::json!({ "config": config }),
                ),
                Ok((_result, message, _config)) => outcome(
                    false,
                    if message.trim().is_empty() {
                        format!("User '{username}' configuration not found")
                    } else {
                        message
                    },
                    serde_json::json!({}),
                ),
                Err(message) => outcome(false, message, serde_json::json!({})),
            }
        }
        "modify_time_left" => {
            let client = match TimekprDbusClient::connect().await {
                Ok(client) => client,
                Err(message) => return outcome(false, message, serde_json::json!({})),
            };
            let op = args
                .get("operation")
                .and_then(|v| v.as_str())
                .unwrap_or("+");
            let secs = args.get("seconds").and_then(|v| v.as_i64()).unwrap_or(0);
            let secs = match i32::try_from(secs) {
                Ok(value) if value >= 0 => value,
                _ => {
                    return outcome(
                        false,
                        "seconds must be a non-negative integer".to_string(),
                        serde_json::json!({}),
                    );
                }
            };

            match client.set_time_left(username, op, secs).await {
                Ok((result, _message)) if result == 0 => outcome(
                    true,
                    format!("Successfully modified time: {op}{secs} seconds"),
                    serde_json::json!({}),
                ),
                Ok((_result, message)) => outcome(
                    false,
                    if message.trim().is_empty() {
                        "Failed to modify time".to_string()
                    } else {
                        message
                    },
                    serde_json::json!({}),
                ),
                Err(message) => outcome(false, message, serde_json::json!({})),
            }
        }
        "set_weekly_time_limits" => {
            let client = match TimekprDbusClient::connect().await {
                Ok(client) => client,
                Err(message) => return outcome(false, message, serde_json::json!({})),
            };
            let schedule = match args.get("schedule").and_then(|v| v.as_object()) {
                Some(schedule) => schedule,
                None => {
                    return outcome(
                        false,
                        "Missing 'schedule' argument".to_string(),
                        serde_json::json!({}),
                    );
                }
            };

            let (allowed_days, day_limits) = match schedule_to_day_limits(schedule) {
                Ok(value) => value,
                Err(message) => return outcome(false, message, serde_json::json!({})),
            };

            if allowed_days.is_empty() {
                return outcome(
                    false,
                    "No allowed days with time limits configured".to_string(),
                    serde_json::json!({}),
                );
            }

            let (days_result, days_message) =
                match client.set_allowed_days(username, &allowed_days).await {
                    Ok(result) => result,
                    Err(message) => return outcome(false, message, serde_json::json!({})),
                };
            if days_result != 0 {
                return outcome(
                    false,
                    if days_message.trim().is_empty() {
                        "Failed to set allowed days".to_string()
                    } else {
                        days_message
                    },
                    serde_json::json!({}),
                );
            }

            let (limits_result, limits_message) =
                match client.set_time_limit_for_days(username, &day_limits).await {
                    Ok(result) => result,
                    Err(message) => return outcome(false, message, serde_json::json!({})),
                };
            if limits_result != 0 {
                return outcome(
                    false,
                    if limits_message.trim().is_empty() {
                        "Failed to set time limits".to_string()
                    } else {
                        limits_message
                    },
                    serde_json::json!({}),
                );
            }

            outcome(
                true,
                "Weekly time limits configured successfully".to_string(),
                serde_json::json!({}),
            )
        }
        "set_allowed_hours" => {
            let client = match TimekprDbusClient::connect().await {
                Ok(client) => client,
                Err(message) => return outcome(false, message, serde_json::json!({})),
            };
            let intervals = match args.get("intervals").and_then(|v| v.as_object()) {
                Some(intervals) => intervals,
                None => {
                    return outcome(
                        false,
                        "Missing 'intervals' argument".to_string(),
                        serde_json::json!({}),
                    );
                }
            };

            let day_order = ["1", "2", "3", "4", "5", "6", "7"];
            let mut success_count = 0;
            let mut total_count = 0;
            let mut errors = Vec::new();

            for day_str in day_order {
                let day_hours = if let Some(hours_val) = intervals.get(day_str) {
                    match parse_day_hours(hours_val, day_str) {
                        Ok(parsed) => parsed,
                        Err(message) => {
                            errors.push(format!("Day {day_str}: {message}"));
                            total_count += 1;
                            continue;
                        }
                    }
                } else {
                    build_full_access_day()
                };

                total_count += 1;

                match client
                    .set_allowed_hours(username, day_str, &day_hours)
                    .await
                {
                    Ok((result, _message)) if result == 0 => {
                        success_count += 1;
                    }
                    Ok((_result, message)) => {
                        errors.push(format!(
                            "Day {day_str}: {}",
                            if message.trim().is_empty() {
                                "Failed to update allowed hours".to_string()
                            } else {
                                message
                            }
                        ));
                    }
                    Err(message) => {
                        errors.push(format!("Day {day_str}: {message}"));
                    }
                }
            }

            if success_count == total_count {
                outcome(
                    true,
                    format!(
                        "Successfully set allowed hours for {success_count}/{total_count} days"
                    ),
                    serde_json::json!({}),
                )
            } else {
                outcome(
                    false,
                    format!("Errors setting allowed hours: {}", errors.join("; ")),
                    serde_json::json!({}),
                )
            }
        }
        "sync_apparmor_policy" => {
            let policies_val = match args.get("policies") {
                Some(policies) => policies,
                None => {
                    return outcome(
                        false,
                        "Missing 'policies' argument".to_string(),
                        serde_json::json!({}),
                    );
                }
            };

            let policies: Vec<apparmor::AppArmorPolicy> =
                match serde_json::from_value(policies_val.clone()) {
                    Ok(policies) => policies,
                    Err(error) => {
                        return outcome(
                            false,
                            format!("Failed to parse policies: {error}"),
                            serde_json::json!({}),
                        );
                    }
                };

            let approval_policy =
                approval_policy::ApprovalPolicy::parse(args.get("approval_policy"));

            match apparmor::sync_user_policy(username, policies, approval_policy).await {
                Ok(message) => outcome(true, message, serde_json::json!({})),
                Err(error) => outcome(false, error, serde_json::json!({})),
            }
        }
        "sync_linux_device_policy" => {
            let payload = linux_device_policy::parse_device_policy(args.get("device_policy"));
            match linux_device_policy::sync_user_policy(username, payload).await {
                Ok(()) => outcome(
                    true,
                    "Linux device policy synchronized".to_string(),
                    serde_json::json!({}),
                ),
                Err(message) => outcome(false, message, serde_json::json!({})),
            }
        }
        "refresh_installed_apps" => outcome(
            true,
            "Installed apps refresh queued".to_string(),
            serde_json::json!({ "queued": true, "linux_username": username }),
        ),
        "sync_screenshot_policy" => {
            match screenshot::apply_screenshot_policy(get_screenshot_policy_handle(), args) {
                Ok(()) => {
                    wake_screenshot_scheduler();
                    outcome(
                        true,
                        "Screenshot policy synchronized".to_string(),
                        serde_json::json!({}),
                    )
                }
                Err(message) => outcome(false, message, serde_json::json!({})),
            }
        }
        "capture_screenshot" => outcome(
            true,
            "Screenshot capture queued".to_string(),
            serde_json::json!({
                "queued": true,
                "linux_username": args
                    .get("linux_username")
                    .and_then(|value| value.as_str())
                    .or_else(|| if username.trim().is_empty() { None } else { Some(username) }),
            }),
        ),
        "unenroll" => {
            if let Err(message) = linux_device_policy::clear_on_unenroll().await {
                eprintln!("Warning: failed to clear linux device policy on unenroll: {message}");
            }
            match clear_agent_enrollment() {
                Ok(()) => outcome(
                    true,
                    "Device unenrolled locally; agent token cleared".to_string(),
                    serde_json::json!({}),
                ),
                Err(message) => outcome(false, message, serde_json::json!({})),
            }
        }
        "show_overlay" => match overlay::show(args, username) {
            Ok(message) => outcome(true, message, serde_json::json!({})),
            Err(message) => outcome(false, message, serde_json::json!({})),
        },
        "dismiss_overlay" => {
            overlay::dismiss();
            outcome(
                true,
                "Guardian Space overlay dismissed".to_string(),
                serde_json::json!({}),
            )
        }
        "detect_hardware_oem" | "audit_hardware_baseline" | "apply_hardware_baseline" => {
            let (success, message, data) = bios_management::handle_command(action, args);
            outcome(success, message, data)
        }
        _ => outcome(
            false,
            format!("Unknown action '{action}'"),
            serde_json::json!({}),
        ),
    }
}
