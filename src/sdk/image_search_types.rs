use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::borrow::Cow;

use super::constants::DEFAULT_IMAGE_SORTS;

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
pub enum ImageSearchSortBy {
    #[default]
    Relevance,
    MostReactions,
    MostDiscussed,
    MostCollected,
    MostBuzz,
    Newest,
    Custom(String),
}

impl ImageSearchSortBy {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Relevance,
            Self::MostReactions,
            Self::MostDiscussed,
            Self::MostCollected,
            Self::MostBuzz,
            Self::Newest,
        ]
    }

    pub fn label(&self) -> Cow<'_, str> {
        match self {
            Self::Relevance => Cow::Borrowed("Relevance"),
            Self::MostReactions => Cow::Borrowed("Most Reactions"),
            Self::MostDiscussed => Cow::Borrowed("Most Discussed"),
            Self::MostCollected => Cow::Borrowed("Most Collected"),
            Self::MostBuzz => Cow::Borrowed("Most Buzz"),
            Self::Newest => Cow::Borrowed("Newest"),
            Self::Custom(value) => Cow::Borrowed(value.as_str()),
        }
    }

    pub fn to_query_value(&self) -> Cow<'_, str> {
        match self {
            Self::Relevance => Cow::Borrowed(DEFAULT_IMAGE_SORTS[0]),
            Self::MostReactions => Cow::Borrowed(DEFAULT_IMAGE_SORTS[1]),
            Self::MostDiscussed => Cow::Borrowed(DEFAULT_IMAGE_SORTS[2]),
            Self::MostCollected => Cow::Borrowed(DEFAULT_IMAGE_SORTS[3]),
            Self::MostBuzz => Cow::Borrowed(DEFAULT_IMAGE_SORTS[4]),
            Self::Newest => Cow::Borrowed(DEFAULT_IMAGE_SORTS[5]),
            Self::Custom(value) => Cow::Borrowed(value.as_str()),
        }
    }

    pub fn from_query_value(value: &str) -> Self {
        if value == DEFAULT_IMAGE_SORTS[0] {
            return Self::Relevance;
        }
        if value == DEFAULT_IMAGE_SORTS[1] {
            return Self::MostReactions;
        }
        if value == DEFAULT_IMAGE_SORTS[2] {
            return Self::MostDiscussed;
        }
        if value == DEFAULT_IMAGE_SORTS[3] {
            return Self::MostCollected;
        }
        if value == DEFAULT_IMAGE_SORTS[4] {
            return Self::MostBuzz;
        }
        if value == DEFAULT_IMAGE_SORTS[5] {
            return Self::Newest;
        }
        Self::Custom(value.to_string())
    }

    pub fn to_meili_sort_value(&self) -> Option<Cow<'_, str>> {
        match self {
            Self::Relevance => None,
            Self::MostReactions => Some(Cow::Borrowed("stats.reactionCountAllTime:desc")),
            Self::MostDiscussed => Some(Cow::Borrowed("stats.commentCountAllTime:desc")),
            Self::MostCollected => Some(Cow::Borrowed("stats.collectedCountAllTime:desc")),
            Self::MostBuzz => Some(Cow::Borrowed("stats.tippedAmountCountAllTime:desc")),
            Self::Newest => Some(Cow::Borrowed("createdAt:desc")),
            Self::Custom(value) => custom_image_sort_to_meili(value).map(Cow::Owned),
        }
    }
}

impl Serialize for ImageSearchSortBy {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.to_query_value().as_ref())
    }
}

impl<'de> Deserialize<'de> for ImageSearchSortBy {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from_query_value(&value))
    }
}

string_enum_with_custom!(ImageMediaType {
    Image => "image",
    Video => "video",
    Audio => "audio",
});

string_enum_with_custom!(ImageAspectRatio {
    Landscape => "Landscape",
    Portrait => "Portrait",
    Square => "Square",
    Unknown => "Unknown",
});

string_enum_with_custom!(ImageTechnique {
    Controlnet => "controlnet",
    Img2Img => "img2img",
    Img2Vid => "img2vid",
    Inpainting => "inpainting",
    Txt2Img => "txt2img",
    Txt2Vid => "txt2vid",
    Vid2Vid => "vid2vid",
    Workflow => "workflow",
});

string_enum_with_custom!(ImageTool {
    A1111 => "A1111",
    AdobeAfterEffects => "Adobe AfterEffects",
    AdobeFirefly => "Adobe Firefly",
    AdobePhotoshop => "Adobe Photoshop",
    AdobePremiere => "Adobe Premiere",
    Civitai => "Civitai",
    ComfyUi => "ComfyUI",
    ChatGpt => "ChatGPT",
    Flux => "Flux",
    Fooocus => "Fooocus",
    Forge => "Forge",
    Gemini => "Gemini",
    Grok => "Grok",
    Invoke => "Invoke",
    Kling => "Kling",
    Krita => "Krita",
    Krea => "KREA",
    LightricksLtxv => "Lightricks LTXV",
    MiniMaxHailuo => "MiniMax / Hailuo",
});

string_enum_with_custom!(ImageBaseModel {
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

fn custom_image_sort_to_meili(value: &str) -> Option<String> {
    value.strip_prefix("images_v6:").map(str::to_string)
}

impl ImageMediaType {
    pub fn all() -> Vec<Self> {
        vec![Self::Image, Self::Video, Self::Audio]
    }

    pub fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.as_query_value())
    }
}

impl ImageAspectRatio {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Landscape,
            Self::Portrait,
            Self::Square,
            Self::Unknown,
        ]
    }

    pub fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.as_query_value())
    }
}

impl ImageTechnique {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Controlnet,
            Self::Img2Img,
            Self::Img2Vid,
            Self::Inpainting,
            Self::Txt2Img,
            Self::Txt2Vid,
            Self::Vid2Vid,
            Self::Workflow,
        ]
    }

    pub fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.as_query_value())
    }
}

impl ImageTool {
    pub fn all() -> Vec<Self> {
        vec![
            Self::A1111,
            Self::AdobeAfterEffects,
            Self::AdobeFirefly,
            Self::AdobePhotoshop,
            Self::AdobePremiere,
            Self::Civitai,
            Self::ComfyUi,
            Self::ChatGpt,
            Self::Flux,
            Self::Fooocus,
            Self::Forge,
            Self::Gemini,
            Self::Grok,
            Self::Invoke,
            Self::Kling,
            Self::Krita,
            Self::Krea,
            Self::LightricksLtxv,
            Self::MiniMaxHailuo,
        ]
    }

    pub fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.as_query_value())
    }
}

impl ImageBaseModel {
    pub fn all() -> Vec<Self> {
        vec![
            Self::Chroma,
            Self::Flux1D,
            Self::Flux1Kontext,
            Self::Flux1Krea,
            Self::Flux1S,
            Self::Flux2D,
            Self::HiDream,
            Self::HunyuanVideo,
            Self::Illustrious,
            Self::Imagen4,
            Self::NanoBanana,
            Self::NoobAi,
            Self::OpenAi,
            Self::Pony,
            Self::PonyV7,
            Self::Qwen,
            Self::Sd14,
            Self::Sd15,
            Self::Sd20,
            Self::Sd3,
            Self::Sd35Large,
            Self::Sdxl10,
            Self::SdxlTurbo,
            Self::Seedream,
            Self::Veo3,
            Self::WanVideo22I2vA14b,
            Self::WanVideo22T2vA14b,
            Self::WanVideo22Ti2v5b,
            Self::WanVideo25I2v,
            Self::WanVideo25T2v,
            Self::ZImageBase,
            Self::ZImageTurbo,
        ]
    }

    pub fn label(&self) -> Cow<'_, str> {
        Cow::Borrowed(self.as_query_value())
    }
}
