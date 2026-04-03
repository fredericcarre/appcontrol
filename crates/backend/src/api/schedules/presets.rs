//! Schedule preset templates.

use axum::Json;
use super::types::PresetInfo;

pub async fn list_presets() -> Json<Vec<PresetInfo>> {
    let presets = vec![
        PresetInfo { id: "daily_7am".into(), label: "Daily at 7:00 AM".into(), description: "Every day at 7 AM".into(), cron: "0 7 * * *".into() },
        PresetInfo { id: "daily_8am".into(), label: "Daily at 8:00 AM".into(), description: "Every day at 8 AM".into(), cron: "0 8 * * *".into() },
        PresetInfo { id: "daily_19h".into(), label: "Daily at 7:00 PM".into(), description: "Every day at 7 PM".into(), cron: "0 19 * * *".into() },
        PresetInfo { id: "daily_22h".into(), label: "Daily at 10:00 PM".into(), description: "Every day at 10 PM".into(), cron: "0 22 * * *".into() },
        PresetInfo { id: "weekdays_7am".into(), label: "Weekdays at 7:00 AM".into(), description: "Monday to Friday at 7 AM".into(), cron: "0 7 * * 1-5".into() },
        PresetInfo { id: "weekdays_8am".into(), label: "Weekdays at 8:00 AM".into(), description: "Monday to Friday at 8 AM".into(), cron: "0 8 * * 1-5".into() },
        PresetInfo { id: "weekdays_19h".into(), label: "Weekdays at 7:00 PM".into(), description: "Monday to Friday at 7 PM".into(), cron: "0 19 * * 1-5".into() },
        PresetInfo { id: "weekdays_22h".into(), label: "Weekdays at 10:00 PM".into(), description: "Monday to Friday at 10 PM".into(), cron: "0 22 * * 1-5".into() },
        PresetInfo { id: "weekly_sunday_3am".into(), label: "Sundays at 3:00 AM".into(), description: "Every Sunday at 3 AM".into(), cron: "0 3 * * 0".into() },
        PresetInfo { id: "weekly_saturday_3am".into(), label: "Saturdays at 3:00 AM".into(), description: "Every Saturday at 3 AM".into(), cron: "0 3 * * 6".into() },
        PresetInfo { id: "monthly_1st_3am".into(), label: "Monthly (1st at 3 AM)".into(), description: "First day of each month at 3 AM".into(), cron: "0 3 1 * *".into() },
        PresetInfo { id: "every_hour".into(), label: "Every hour".into(), description: "At the start of every hour".into(), cron: "0 * * * *".into() },
        PresetInfo { id: "every_30min".into(), label: "Every 30 minutes".into(), description: "Every 30 minutes".into(), cron: "*/30 * * * *".into() },
    ];
    Json(presets)
}
