use civitai_cli::sdk::{
    ImageAspectRatio, ImageBaseModel, ImageMediaType, ImageSearchSortBy, ImageSearchState,
    ModelBaseModel, ModelSearchSortBy, ModelSearchState, ModelType,
};
use std::collections::BTreeSet;

use super::filters::period_to_created_at;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchFormMode {
    Quick,
    Builder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchFormSection {
    Query,
    Sort,
    Period,
    Type,
    Tag,
    BaseModel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchPeriod {
    AllTime,
    Year,
    Month,
    Week,
    Day,
}

impl SearchPeriod {
    pub fn all() -> Vec<Self> {
        vec![
            Self::AllTime,
            Self::Year,
            Self::Month,
            Self::Week,
            Self::Day,
        ]
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::AllTime => "AllTime",
            Self::Year => "Year",
            Self::Month => "Month",
            Self::Week => "Week",
            Self::Day => "Day",
        }
    }
}

#[derive(Clone)]
pub struct SearchFormState {
    pub query: String,
    pub mode: SearchFormMode,
    pub focused_section: SearchFormSection,
    pub sort_options: Vec<ModelSearchSortBy>,
    pub selected_sort: usize,
    pub type_options: Vec<ModelType>,
    pub type_cursor: usize,
    pub selected_types: BTreeSet<ModelType>,
    pub tag_query: String,
    pub base_options: Vec<ModelBaseModel>,
    pub base_cursor: usize,
    pub selected_base_models: BTreeSet<ModelBaseModel>,
    pub periods: Vec<SearchPeriod>,
    pub selected_period: usize,
}

pub struct SettingsFormState {
    pub editing: bool,
    pub focused_field: usize,
    pub input_buffer: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaRenderRequest {
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageSearchFormSection {
    Query,
    Sort,
    Period,
    MediaType,
    Tag,
    BaseModel,
    AspectRatio,
}

#[derive(Clone)]
pub struct ImageSearchFormState {
    pub query: String,
    pub mode: SearchFormMode,
    pub focused_section: ImageSearchFormSection,
    pub sort_options: Vec<ImageSearchSortBy>,
    pub selected_sort: usize,
    pub periods: Vec<SearchPeriod>,
    pub selected_period: usize,
    pub media_type_options: Vec<ImageMediaType>,
    pub media_type_cursor: usize,
    pub selected_media_types: BTreeSet<String>,
    pub tag_query: String,
    pub base_options: Vec<ImageBaseModel>,
    pub base_cursor: usize,
    pub selected_base_models: BTreeSet<String>,
    pub aspect_ratio_options: Vec<ImageAspectRatio>,
    pub aspect_ratio_cursor: usize,
    pub selected_aspect_ratios: BTreeSet<String>,
    pub linked_model_version_id: Option<u64>,
}

impl SettingsFormState {
    pub fn new() -> Self {
        Self {
            editing: false,
            focused_field: 0,
            input_buffer: String::new(),
        }
    }
}

impl ImageSearchFormState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            mode: SearchFormMode::Quick,
            focused_section: ImageSearchFormSection::Query,
            sort_options: ImageSearchSortBy::all(),
            selected_sort: 0,
            periods: SearchPeriod::all(),
            selected_period: 0,
            media_type_options: ImageMediaType::all(),
            media_type_cursor: 0,
            selected_media_types: {
                let mut set = BTreeSet::new();
                set.insert(ImageMediaType::Image.as_query_value().to_string());
                set
            },
            tag_query: String::new(),
            base_options: ImageBaseModel::all(),
            base_cursor: 0,
            selected_base_models: BTreeSet::new(),
            aspect_ratio_options: ImageAspectRatio::all(),
            aspect_ratio_cursor: 0,
            selected_aspect_ratios: BTreeSet::new(),
            linked_model_version_id: None,
        }
    }

    pub fn build_options(&self) -> ImageSearchState {
        ImageSearchState {
            query: (!self.query.trim().is_empty()).then(|| self.query.trim().to_string()),
            sort_by: self
                .sort_options
                .get(self.selected_sort)
                .cloned()
                .unwrap_or_default(),
            media_types: self
                .selected_media_types
                .iter()
                .map(|value| ImageMediaType::from_query_value(value))
                .collect(),
            tags: self
                .tag_query
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
                .collect(),
            base_models: self
                .selected_base_models
                .iter()
                .map(|value| ImageBaseModel::from_query_value(value))
                .collect(),
            aspect_ratios: self
                .selected_aspect_ratios
                .iter()
                .map(|value| ImageAspectRatio::from_query_value(value))
                .collect(),
            created_at: self
                .periods
                .get(self.selected_period)
                .and_then(|period| period_to_created_at(period.label())),
            limit: Some(50),
            extras: self
                .linked_model_version_id
                .map(|id| vec![("modelVersionId".to_string(), id.to_string())])
                .unwrap_or_default(),
            ..Default::default()
        }
    }

    pub fn begin_quick_search(&mut self) {
        self.mode = SearchFormMode::Quick;
        self.focused_section = ImageSearchFormSection::Query;
    }

    pub fn begin_builder(&mut self) {
        self.mode = SearchFormMode::Builder;
        if self.focused_section == ImageSearchFormSection::Query {
            self.focused_section = ImageSearchFormSection::Sort;
        }
    }

    pub fn set_linked_model_version(&mut self, version_id: Option<u64>) {
        self.linked_model_version_id = version_id;
    }
}

impl SearchFormState {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            mode: SearchFormMode::Quick,
            focused_section: SearchFormSection::Query,
            sort_options: ModelSearchSortBy::all(),
            selected_sort: ModelSearchSortBy::all()
                .iter()
                .position(|sort| *sort == ModelSearchSortBy::Relevance)
                .unwrap_or(0),
            type_options: ModelType::all(),
            type_cursor: 0,
            selected_types: BTreeSet::new(),
            tag_query: String::new(),
            base_options: ModelBaseModel::all(),
            base_cursor: 0,
            selected_base_models: BTreeSet::new(),
            periods: SearchPeriod::all(),
            selected_period: 0,
        }
    }

    pub fn build_options(&self) -> ModelSearchState {
        ModelSearchState {
            query: (!self.query.trim().is_empty()).then(|| self.query.trim().to_string()),
            sort_by: self
                .sort_options
                .get(self.selected_sort)
                .cloned()
                .unwrap_or_default(),
            tags: self
                .tag_query
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| value.to_string())
                .collect(),
            base_models: self.selected_base_models.iter().cloned().collect(),
            types: self.selected_types.iter().cloned().collect(),
            created_at: self
                .periods
                .get(self.selected_period)
                .map(|period| period_to_created_at(period.label()))
                .unwrap_or(None),
            limit: Some(50),
            ..Default::default()
        }
    }

    pub fn begin_quick_search(&mut self) {
        self.mode = SearchFormMode::Quick;
        self.focused_section = SearchFormSection::Query;
    }

    pub fn begin_builder(&mut self) {
        self.mode = SearchFormMode::Builder;
        if self.focused_section == SearchFormSection::Query {
            self.focused_section = SearchFormSection::Sort;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_model_search_options_from_form_state() {
        let mut form = SearchFormState::new();
        form.query = " flux ".to_string();
        form.tag_query = "anime, detailed, ".to_string();
        form.selected_types.insert(ModelType::Lora);
        form.selected_base_models.insert(ModelBaseModel::Flux1D);
        form.selected_period = 2;

        let options = form.build_options();

        assert_eq!(options.query.as_deref(), Some("flux"));
        assert_eq!(
            options.tags,
            vec!["anime".to_string(), "detailed".to_string()]
        );
        assert_eq!(options.types, vec![ModelType::Lora]);
        assert_eq!(options.base_models, vec![ModelBaseModel::Flux1D]);
        assert!(options.created_at.is_some());
        assert_eq!(options.limit, Some(50));
    }

    #[test]
    fn builds_image_search_options_with_linked_model_version() {
        let mut form = ImageSearchFormState::new();
        form.query = "portrait".to_string();
        form.tag_query = "studio, cinematic".to_string();
        form.selected_base_models
            .insert(ImageBaseModel::Flux1D.as_query_value().to_string());
        form.selected_aspect_ratios
            .insert(ImageAspectRatio::Landscape.as_query_value().to_string());
        form.set_linked_model_version(Some(42));

        let options = form.build_options();

        assert_eq!(options.query.as_deref(), Some("portrait"));
        assert_eq!(
            options.tags,
            vec!["studio".to_string(), "cinematic".to_string()]
        );
        assert_eq!(options.base_models, vec![ImageBaseModel::Flux1D]);
        assert_eq!(options.aspect_ratios, vec![ImageAspectRatio::Landscape]);
        assert_eq!(
            options.extras,
            vec![("modelVersionId".to_string(), "42".to_string())]
        );
    }

    #[test]
    fn switching_to_builder_moves_focus_off_query() {
        let mut form = ImageSearchFormState::new();

        form.begin_builder();

        assert_eq!(form.mode, SearchFormMode::Builder);
        assert_eq!(form.focused_section, ImageSearchFormSection::Sort);
    }
}
