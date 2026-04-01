use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

use super::constants::DEFAULT_MODEL_SORTS;

macro_rules! string_enum_with_custom {
    ($name:ident { $($variant:ident => $value:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub enum $name {
            #[default]
            $($variant,)+
            Custom(String),
        }

        impl $name {
            pub fn as_query_value(&self) -> &str {
                match self {
                    $(Self::$variant => $value,)+
                    Self::Custom(value) => value.as_str(),
                }
            }

            pub fn from_query_value(value: &str) -> Self {
                match value {
                    $($value => Self::$variant,)+
                    other => Self::Custom(other.to_string()),
                }
            }

            pub fn custom(value: impl Into<String>) -> Self {
                Self::Custom(value.into())
            }
        }

        impl Serialize for $name {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                serializer.serialize_str(self.as_query_value())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let value = String::deserialize(deserializer)?;
                Ok(Self::from_query_value(&value))
            }
        }
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ModelSearchSortBy {
    #[default]
    Relevance,
    HighestRated,
    MostDownloaded,
    MostLiked,
    MostDiscussed,
    MostCollected,
    MostBuzz,
    Newest,
    Custom(String),
}

impl ModelSearchSortBy {
    pub fn to_query_value(&self) -> Cow<'_, str> {
        match self {
            Self::Relevance => Cow::Borrowed(DEFAULT_MODEL_SORTS[0]),
            Self::HighestRated => Cow::Borrowed(DEFAULT_MODEL_SORTS[1]),
            Self::MostDownloaded => Cow::Borrowed(DEFAULT_MODEL_SORTS[2]),
            Self::MostLiked => Cow::Borrowed(DEFAULT_MODEL_SORTS[3]),
            Self::MostDiscussed => Cow::Borrowed(DEFAULT_MODEL_SORTS[4]),
            Self::MostCollected => Cow::Borrowed(DEFAULT_MODEL_SORTS[5]),
            Self::MostBuzz => Cow::Borrowed(DEFAULT_MODEL_SORTS[6]),
            Self::Newest => Cow::Borrowed(DEFAULT_MODEL_SORTS[7]),
            Self::Custom(value) => Cow::Borrowed(value.as_str()),
        }
    }

    pub fn from_query_value(value: &str) -> Self {
        if value == DEFAULT_MODEL_SORTS[0] {
            return Self::Relevance;
        }
        if value == DEFAULT_MODEL_SORTS[1] {
            return Self::HighestRated;
        }
        if value == DEFAULT_MODEL_SORTS[2] {
            return Self::MostDownloaded;
        }
        if value == DEFAULT_MODEL_SORTS[3] {
            return Self::MostLiked;
        }
        if value == DEFAULT_MODEL_SORTS[4] {
            return Self::MostDiscussed;
        }
        if value == DEFAULT_MODEL_SORTS[5] {
            return Self::MostCollected;
        }
        if value == DEFAULT_MODEL_SORTS[6] {
            return Self::MostBuzz;
        }
        if value == DEFAULT_MODEL_SORTS[7] {
            return Self::Newest;
        }
        Self::Custom(value.to_string())
    }

    pub fn to_meili_sort_value(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Relevance => None,
            Self::HighestRated => Some(Cow::Borrowed("metrics.thumbsUpCount:desc")),
            Self::MostDownloaded => Some(Cow::Borrowed("metrics.downloadCount:desc")),
            Self::MostLiked => Some(Cow::Borrowed("metrics.favoriteCount:desc")),
            Self::MostDiscussed => Some(Cow::Borrowed("metrics.commentCount:desc")),
            Self::MostCollected => Some(Cow::Borrowed("metrics.collectedCount:desc")),
            Self::MostBuzz => Some(Cow::Borrowed("metrics.tippedAmountCount:desc")),
            Self::Newest => Some(Cow::Borrowed("createdAt:desc")),
            Self::Custom(value) => custom_model_sort_to_meili(value).map(Cow::Owned),
        }
    }
}

impl Serialize for ModelSearchSortBy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_query_value().as_ref())
    }
}

impl<'de> Deserialize<'de> for ModelSearchSortBy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_query_value(&value))
    }
}

string_enum_with_custom!(ModelBaseModel {
    Chroma => "Chroma",
    Flux1D => "Flux.1 D",
    Flux1Kontext => "Flux.1 Kontext",
    Flux1Krea => "Flux.1 Krea",
    Flux1S => "Flux.1 S",
    Flux2D => "Flux.2 D",
    HiDream => "HiDream",
    HunyuanVideo => "Hunyuan Video",
    Illustrious => "Illustrious",
    Imagen4 => "Imagen4",
    NanoBanana => "Nano Banana",
    NoobAi => "NoobAI",
    OpenAi => "OpenAI",
    Pony => "Pony",
    PonyV7 => "Pony V7",
    Qwen => "Qwen",
    Sd14 => "SD 1.4",
    Sd15 => "SD 1.5",
    Sd20 => "SD 2.0",
    Sd3 => "SD 3",
    Sd35Large => "SD 3.5 Large",
    Sdxl10 => "SDXL 1.0",
    SdxlTurbo => "SDXL Turbo",
    Seedream => "Seedream",
    Veo3 => "Veo 3",
    WanVideo22I2vA14b => "Wan Video 2.2 I2V-A14B",
    WanVideo22T2vA14b => "Wan Video 2.2 T2V-A14B",
    WanVideo22Ti2v5b => "Wan Video 2.2 TI2V-5B",
    WanVideo25I2v => "Wan Video 2.5 I2V",
    WanVideo25T2v => "Wan Video 2.5 T2V",
    ZImageBase => "ZImageBase",
    ZImageTurbo => "ZImageTurbo",
});

string_enum_with_custom!(ModelType {
    Checkpoint => "Checkpoint",
    Lora => "LORA",
    LoCon => "LoCon",
    TextualInversion => "TextualInversion",
    Hypernetwork => "Hypernetwork",
    AestheticGradient => "AestheticGradient",
    Controlnet => "Controlnet",
    Poses => "Poses",
    MotionModule => "MotionModule",
    Vae => "VAE",
    Upscaler => "Upscaler",
    Workflows => "Workflows",
    Tool => "Tool",
    Wildcards => "Wildcards",
    Detection => "Detection",
    DoRa => "DoRA",
    Other => "Other",
});

string_enum_with_custom!(ModelCheckpointType {
    Merge => "Merge",
    Trained => "Trained",
    Pruned => "Pruned",
});

string_enum_with_custom!(ModelFileFormat {
    SafeTensor => "SafeTensor",
    PickleTensor => "PickleTensor",
    GGUF => "GGUF",
    CKPT => "CKPT",
    Diffusers => "Diffusers",
    Other => "Other",
});

string_enum_with_custom!(ModelCategory {
    Character => "character",
    Style => "style",
    Concept => "concept",
    Clothing => "clothing",
    Poses => "poses",
    Background => "background",
    Tool => "tool",
});

fn custom_model_sort_to_meili(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed == DEFAULT_MODEL_SORTS[0] {
        return None;
    }

    if let Some(stripped) = trimmed.strip_prefix(&format!("{}:", DEFAULT_MODEL_SORTS[0])) {
        return Some(stripped.to_string());
    }

    Some(trimmed.to_string())
}
