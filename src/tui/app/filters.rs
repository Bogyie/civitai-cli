use civitai_cli::sdk::{ModelBaseModel, ModelSearchSortBy, ModelType, SearchModelHit as Model};
use std::collections::BTreeSet;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::tui::app::SearchPeriod;
use crate::tui::model::{default_base_model, model_metrics, model_name, model_versions};

pub(super) fn has_displayable_model_version(model: &Model) -> bool {
    !model_versions(model).is_empty() || model.primary_model_version_id().is_some()
}

pub(super) fn period_to_created_at(period: &str) -> Option<String> {
    let end = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs();

    let start = match period {
        "Day" => end.saturating_sub(24 * 60 * 60),
        "Week" => end.saturating_sub(7 * 24 * 60 * 60),
        "Month" => end.saturating_sub(30 * 24 * 60 * 60),
        "Year" => end.saturating_sub(365 * 24 * 60 * 60),
        _ => return None,
    };

    Some(format!("{start}-{end}"))
}

pub(super) fn liked_model_matches_query(model: &Model, query: &str) -> bool {
    query.is_empty() || model_name(model).to_ascii_lowercase().contains(query)
}

pub(super) fn liked_model_matches_type(
    model: &Model,
    selected_types: &BTreeSet<ModelType>,
) -> bool {
    if selected_types.is_empty() {
        return true;
    }

    let Some(model_type) = model.r#type.as_deref() else {
        return false;
    };
    let normalized = model_type.to_ascii_lowercase();
    selected_types.iter().any(|item| {
        normalized == item.as_query_value().to_ascii_lowercase()
            || normalized == item.label().to_ascii_lowercase()
    })
}

pub(super) fn liked_model_matches_base_model(
    model: &Model,
    selected_base_models: &BTreeSet<ModelBaseModel>,
) -> bool {
    if selected_base_models.is_empty() {
        return true;
    }

    let Some(base_model) = default_base_model(model) else {
        return false;
    };
    let normalized = base_model.to_ascii_lowercase();
    selected_base_models.iter().any(|item| {
        normalized == item.as_query_value().to_ascii_lowercase()
            || normalized == item.label().to_ascii_lowercase()
    })
}

pub(super) fn liked_model_matches_period(model: &Model, period: Option<&SearchPeriod>) -> bool {
    let Some(period) = period else {
        return true;
    };
    if *period == SearchPeriod::AllTime {
        return true;
    }

    let Some(model_ts) = model.last_version_at_unix else {
        return true;
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| Duration::from_secs(0))
        .as_secs() as i64;
    let window = match period {
        SearchPeriod::Day => 24 * 60 * 60,
        SearchPeriod::Week => 7 * 24 * 60 * 60,
        SearchPeriod::Month => 30 * 24 * 60 * 60,
        SearchPeriod::Year => 365 * 24 * 60 * 60,
        SearchPeriod::AllTime => return true,
    };

    model_ts >= now.saturating_sub(window)
}

pub(super) fn sort_liked_models(items: &mut [Model], sort_by: &ModelSearchSortBy) {
    match sort_by {
        ModelSearchSortBy::Relevance => {}
        ModelSearchSortBy::HighestRated => items.sort_by(|a, b| {
            model_metrics(b)
                .rating
                .total_cmp(&model_metrics(a).rating)
                .then_with(|| {
                    model_metrics(b)
                        .rating_count
                        .cmp(&model_metrics(a).rating_count)
                })
        }),
        ModelSearchSortBy::MostDownloaded => items.sort_by(|a, b| {
            model_metrics(b)
                .download_count
                .cmp(&model_metrics(a).download_count)
        }),
        ModelSearchSortBy::MostLiked => items.sort_by(|a, b| {
            model_metrics(b)
                .favorite_count
                .cmp(&model_metrics(a).favorite_count)
                .then_with(|| {
                    model_metrics(b)
                        .thumbs_up_count
                        .cmp(&model_metrics(a).thumbs_up_count)
                })
        }),
        ModelSearchSortBy::MostDiscussed => items.sort_by(|a, b| {
            model_metrics(b)
                .comment_count
                .cmp(&model_metrics(a).comment_count)
        }),
        ModelSearchSortBy::MostCollected => items.sort_by(|a, b| {
            model_metrics(b)
                .collected_count
                .cmp(&model_metrics(a).collected_count)
        }),
        ModelSearchSortBy::MostBuzz => items.sort_by(|a, b| {
            model_metrics(b)
                .tipped_amount_count
                .cmp(&model_metrics(a).tipped_amount_count)
        }),
        ModelSearchSortBy::Newest => items.sort_by(|a, b| {
            b.last_version_at_unix
                .unwrap_or_default()
                .cmp(&a.last_version_at_unix.unwrap_or_default())
        }),
        ModelSearchSortBy::Custom(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn model(value: serde_json::Value) -> Model {
        serde_json::from_value(value).expect("valid model fixture")
    }

    #[test]
    fn creates_expected_created_at_window() {
        let range = period_to_created_at("Week").expect("week range");
        let (start, end) = range.split_once('-').expect("range");
        let start = start.parse::<u64>().expect("start");
        let end = end.parse::<u64>().expect("end");

        assert_eq!(end.saturating_sub(start), 7 * 24 * 60 * 60);
    }

    #[test]
    fn filters_liked_models_by_type_and_base_model() {
        let model = model(json!({
            "id": 1,
            "name": "Flux Lora",
            "type": "LORA",
            "version": {
                "id": 10,
                "baseModel": "Flux.1 D"
            }
        }));

        let type_filter = BTreeSet::from([ModelType::Lora]);
        let base_filter = BTreeSet::from([ModelBaseModel::Flux1D]);

        assert!(liked_model_matches_type(&model, &type_filter));
        assert!(liked_model_matches_base_model(&model, &base_filter));
    }

    #[test]
    fn filters_liked_models_by_recency() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_secs() as i64;
        let fresh = model(json!({ "id": 1, "lastVersionAtUnix": now - 60 }));
        let stale = model(json!({ "id": 2, "lastVersionAtUnix": now - (10 * 24 * 60 * 60) }));

        assert!(liked_model_matches_period(&fresh, Some(&SearchPeriod::Day)));
        assert!(!liked_model_matches_period(
            &stale,
            Some(&SearchPeriod::Day)
        ));
    }

    #[test]
    fn sorts_liked_models_by_download_count() {
        let mut items = vec![
            model(json!({ "id": 1, "metrics": { "downloadCount": 5 } })),
            model(json!({ "id": 2, "metrics": { "downloadCount": 99 } })),
            model(json!({ "id": 3, "metrics": { "downloadCount": 42 } })),
        ];

        sort_liked_models(&mut items, &ModelSearchSortBy::MostDownloaded);

        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            vec![2, 3, 1]
        );
    }
}
