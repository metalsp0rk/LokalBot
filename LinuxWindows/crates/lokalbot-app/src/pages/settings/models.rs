//! Models (`ModelsView.swift`, `ModelStackOverviewView.swift`,
//! `ModelConnectionsView.swift`): the three model roles, presets, remote
//! servers, storage, the processing budget and server connections.
//!
//! Model names and sizes come from a small local table until the model
//! catalog in `lokalbot-ai` is available to the app.

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Div, Entity, FontWeight, IntoElement as _,
    ParentElement as _, SharedString, Styled as _, Subscription, Window, div, px,
};
use lokalbot_core::Settings;
use lokalbot_core::settings::{GenerationBudget, ReasoningLevel, SummarizerBackend};

use super::{Section, SettingsPage};
use crate::state::{AppData, AppEvent};
use crate::theme::{LbThemeExt as _, ty};
use crate::ui::{self, Ico, settings_row};

/// A model the roles can use.
pub struct ModelInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub bytes: u64,
}

pub const SPEECH_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "qwen3-asr-1.7b",
        name: "Qwen3-ASR 1.7B",
        bytes: 2_463_307_541,
    },
    ModelInfo {
        id: "qwen3-asr-0.6b",
        name: "Qwen3-ASR 0.6B",
        bytes: 708_236_945,
    },
    ModelInfo {
        id: "sense-voice",
        name: "SenseVoice (zh/ja/ko/yue/en)",
        bytes: 165_783_878,
    },
    ModelInfo {
        id: "gigaam-russian",
        name: "GigaAM (Russian)",
        bytes: 163_286_197,
    },
];

pub const TEXT_MODELS: &[ModelInfo] = &[
    ModelInfo {
        id: "qwen3.5-0.8b",
        name: "Qwen3.5 0.8B",
        bytes: 532_517_120,
    },
    ModelInfo {
        id: "lfm2.5-1.2b-instruct",
        name: "LFM2.5 1.2B Instruct",
        bytes: 730_895_584,
    },
    ModelInfo {
        id: "qwen3.5-2b",
        name: "Qwen3.5 2B",
        bytes: 1_280_835_840,
    },
    ModelInfo {
        id: "lfm2.5-2.6b",
        name: "LFM2.5 2.6B Reasoning",
        bytes: 1_674_454_848,
    },
    ModelInfo {
        id: "qwen3.5-4b",
        name: "Qwen3.5 4B",
        bytes: 2_740_937_888,
    },
    ModelInfo {
        id: "gemma4-e2b-base-q6",
        name: "Gemma 4 E2B Base (Autocomplete)",
        bytes: 3_845_328_608,
    },
    ModelInfo {
        id: "lfm2.5-8b-a1b",
        name: "LFM2.5 8B (MoE)",
        bytes: 5_155_564_768,
    },
    ModelInfo {
        id: "gemma4-12b",
        name: "Gemma 4 12B",
        bytes: 7_121_860_000,
    },
    ModelInfo {
        id: "qwen3.6-27b",
        name: "Qwen3.6 27B",
        bytes: 16_817_244_384,
    },
    ModelInfo {
        id: "qwen3.6-35b-a3b",
        name: "Qwen 3.6 · 35B-A3B",
        bytes: 17_730_509_792,
    },
];

/// Autocomplete models are small and quick (`Autocomplete picker rules`).
const AUTOCOMPLETE_MODELS: [&str; 5] = [
    "qwen3.5-0.8b",
    "lfm2.5-1.2b-instruct",
    "qwen3.5-2b",
    "qwen3.5-4b",
    "gemma4-e2b-base-q6",
];

pub fn model_name(id: &str) -> String {
    SPEECH_MODELS
        .iter()
        .chain(TEXT_MODELS)
        .find(|m| m.id == id)
        .map(|m| m.name.to_string())
        .unwrap_or_else(|| id.to_string())
}

fn model_bytes(id: &str) -> u64 {
    SPEECH_MODELS
        .iter()
        .chain(TEXT_MODELS)
        .find(|m| m.id == id)
        .map(|m| m.bytes)
        .unwrap_or(0)
}

/// "2.7 GB", "730 MB".
pub fn size_label(bytes: u64) -> String {
    let gb = bytes as f64 / 1_000_000_000.0;
    if gb >= 1.0 {
        format!("{gb:.1} GB")
    } else {
        format!("{} MB", (bytes as f64 / 1_000_000.0).round())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Balanced,
    Lightweight,
}

impl Preset {
    pub const ALL: [Preset; 2] = [Preset::Balanced, Preset::Lightweight];

    pub fn title(self) -> &'static str {
        match self {
            Preset::Balanced => "Balanced local",
            Preset::Lightweight => "Lightweight local",
        }
    }

    fn subtitle(self) -> &'static str {
        match self {
            Preset::Balanced => "Balanced local models for everyday work.",
            Preset::Lightweight => "Smaller downloads and a lighter memory footprint.",
        }
    }

    /// Transcribe, Think and Autocomplete model ids.
    fn models(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Preset::Balanced => ("qwen3-asr-1.7b", "qwen3.5-4b", "gemma4-e2b-base-q6"),
            Preset::Lightweight => ("qwen3-asr-0.6b", "qwen3.5-0.8b", "lfm2.5-1.2b-instruct"),
        }
    }

    fn summary(self) -> String {
        let (speech, think, autocomplete) = self.models();
        format!(
            "Transcribe: {}, Think: {}, Autocomplete: {}",
            model_name(speech),
            model_name(think),
            model_name(autocomplete)
        )
    }

    pub fn matches(self, settings: &Settings) -> bool {
        let (speech, think, autocomplete) = self.models();
        settings.summarizer_backend == SummarizerBackend::BuiltIn
            && settings.transcription_model == speech
            && settings.built_in_model_id == think
            && settings.cotyping_built_in_model_id == autocomplete
    }

    fn apply(self, settings: &mut Settings) {
        let (speech, think, autocomplete) = self.models();
        settings.summarizer_backend = SummarizerBackend::BuiltIn;
        settings.transcription_model = speech.into();
        settings.built_in_model_id = think.into();
        settings.cotyping_built_in_model_id = autocomplete.into();
    }
}

fn budget_label(budget: GenerationBudget) -> &'static str {
    match budget {
        GenerationBudget::Conservative => "Conservative",
        GenerationBudget::Standard => "Default",
        GenerationBudget::Generous => "Generous",
        GenerationBudget::Unlimited => "Unlimited",
    }
}

fn budget_detail(budget: GenerationBudget) -> &'static str {
    match budget {
        GenerationBudget::Conservative => {
            "Half the normal time and tokens. Long meetings may need a second pass."
        }
        GenerationBudget::Standard => {
            "Balanced limits that grow with meeting length, so most meetings finish in one pass."
        }
        GenerationBudget::Generous => {
            "Three times the normal limits for very long meetings or slower servers."
        }
        GenerationBudget::Unlimited => "Runs until finished, with a 6-hour safety stop.",
    }
}

const BUDGETS: [GenerationBudget; 4] = [
    GenerationBudget::Conservative,
    GenerationBudget::Standard,
    GenerationBudget::Generous,
    GenerationBudget::Unlimited,
];

const REASONING: [(ReasoningLevel, &str); 6] = [
    (ReasoningLevel::Automatic, "Automatic"),
    (ReasoningLevel::Off, "Off"),
    (ReasoningLevel::Low, "Low"),
    (ReasoningLevel::Medium, "Medium"),
    (ReasoningLevel::High, "High"),
    (ReasoningLevel::Max, "Max"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ModelsPage {
    Active,
    Connections,
}

impl ModelsPage {
    pub fn for_setting(id: &str) -> Self {
        if id == "settings.summarizerBackend" {
            ModelsPage::Connections
        } else {
            ModelsPage::Active
        }
    }
}

/// The Models subpage and the connection form drafts.
pub struct ModelsState {
    pub page: ModelsPage,
    /// The server form shown: Ollama or OpenAI-compatible.
    connection: SummarizerBackend,
    openai_url: Entity<InputState>,
    openai_model: Entity<InputState>,
    api_key: Entity<InputState>,
    ollama_url: Entity<InputState>,
    ollama_model: Entity<InputState>,
    saved: Option<SharedString>,
}

impl ModelsState {
    pub fn new(settings: &Settings, window: &mut Window, cx: &mut Context<SettingsPage>) -> Self {
        let field = |placeholder: &'static str,
                     value: &str,
                     window: &mut Window,
                     cx: &mut Context<SettingsPage>| {
            let value = value.to_string();
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(placeholder)
                    .default_value(value)
            })
        };
        Self {
            page: ModelsPage::Active,
            connection: if settings.summarizer_backend == SummarizerBackend::Ollama {
                SummarizerBackend::Ollama
            } else {
                SummarizerBackend::OpenAICompatible
            },
            openai_url: field(
                "https://example.com/v1",
                &settings.openai_base_url,
                window,
                cx,
            ),
            openai_model: field(
                "Provider model identifier",
                &settings.openai_model,
                window,
                cx,
            ),
            api_key: cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder("API key (optional)")
                    .masked(true)
            }),
            ollama_url: field(
                "http://localhost:11434",
                &settings.ollama_base_url,
                window,
                cx,
            ),
            ollama_model: field("Ollama model name", &settings.ollama_model, window, cx),
            saved: None,
        }
    }

    pub fn subscribe(
        state: &Self,
        window: &mut Window,
        cx: &mut Context<SettingsPage>,
    ) -> Vec<Subscription> {
        [
            &state.openai_url,
            &state.openai_model,
            &state.ollama_url,
            &state.ollama_model,
        ]
        .into_iter()
        .map(|input| {
            cx.subscribe_in(input, window, |page, _, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::Change) {
                    page.models.saved = None;
                    cx.notify();
                }
            })
        })
        .collect()
    }

    /// Follows connection settings changed elsewhere, unless the user is editing.
    pub fn sync(
        &mut self,
        old: &Settings,
        new: &Settings,
        window: &mut Window,
        cx: &mut Context<SettingsPage>,
    ) {
        for (input, before, after) in [
            (&self.openai_url, &old.openai_base_url, &new.openai_base_url),
            (&self.openai_model, &old.openai_model, &new.openai_model),
            (&self.ollama_url, &old.ollama_base_url, &new.ollama_base_url),
            (&self.ollama_model, &old.ollama_model, &new.ollama_model),
        ] {
            if before != after {
                super::sync_input(input, after, window, cx);
            }
        }
    }

    fn draft(&self, cx: &App) -> (String, String) {
        let (url, model) = match self.connection {
            SummarizerBackend::Ollama => (&self.ollama_url, &self.ollama_model),
            _ => (&self.openai_url, &self.openai_model),
        };
        (
            url.read(cx).value().trim().to_string(),
            model.read(cx).value().trim().to_string(),
        )
    }

    fn has_changes(&self, settings: &Settings, cx: &App) -> bool {
        let (url, model) = self.draft(cx);
        match self.connection {
            SummarizerBackend::Ollama => {
                url != settings.ollama_base_url || model != settings.ollama_model
            }
            _ => url != settings.openai_base_url || model != settings.openai_model,
        }
    }
}

impl SettingsPage {
    pub(super) fn models_sections(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Section> {
        match self.models.page {
            ModelsPage::Active => self.active_models(cx),
            ModelsPage::Connections => self.connections(window, cx),
        }
    }

    fn active_models(&mut self, cx: &mut Context<Self>) -> Vec<Section> {
        let (settings, status, downloads, root) = {
            let data = self.data.read(cx);
            (
                data.settings.clone(),
                data.models.clone(),
                data.downloads.clone(),
                data.library.paths().clone(),
            )
        };
        let lb = cx.lb().clone();
        let local = settings.think_is_local();
        let setup = Preset::ALL
            .iter()
            .find(|p| p.matches(&settings))
            .map(|p| p.title())
            .unwrap_or("Custom");
        let location = if local {
            "On this PC"
        } else {
            "Local + remote"
        };

        let summary = Section::new(None).row(
            "settings.models",
            settings_row::content_row(
                "settings.models",
                h_flex()
                    .gap(px(18.))
                    .items_center()
                    .child(
                        div()
                            .text_size(ty::BODY)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(lb.text)
                            .child("Current setup"),
                    )
                    .child(
                        div()
                            .text_size(ty::BODY)
                            .text_color(lb.text_secondary)
                            .child(format!("{setup} · {location}")),
                    ),
                self.is_focused("settings.models"),
                cx,
            ),
        );

        // Core roles.
        let think_name = match settings.summarizer_backend {
            SummarizerBackend::BuiltIn => model_name(&settings.built_in_model_id),
            SummarizerBackend::Ollama => {
                if settings.ollama_model.is_empty() {
                    "Ollama".into()
                } else {
                    settings.ollama_model.clone()
                }
            }
            SummarizerBackend::OpenAICompatible => {
                if settings.openai_model.is_empty() {
                    "OpenAI-compatible server".into()
                } else {
                    settings.openai_model.clone()
                }
            }
        };
        let think_location = match settings.summarizer_backend {
            SummarizerBackend::BuiltIn => None,
            SummarizerBackend::Ollama => Some(settings.ollama_base_url.clone()),
            SummarizerBackend::OpenAICompatible => Some(settings.openai_base_url.clone()),
        };
        let mut roles = Section::new("Core Roles");
        roles = roles.row(
            "settings.transcriptionModel",
            self.role_row(
                RoleRow {
                    id: "settings.transcriptionModel",
                    icon: Ico::AudioLines,
                    title: "Transcribe",
                    detail: "Meeting audio to text",
                    model: model_name(&settings.transcription_model),
                    remote: None,
                    ready: status.transcription_ready,
                    downloading: downloads
                        .get(&settings.transcription_model)
                        .map(|d| d.fraction()),
                },
                RoleChoices::Speech,
                cx,
            ),
        );
        roles = roles.row(
            "settings.builtInModelID",
            self.role_row(
                RoleRow {
                    id: "settings.builtInModelID",
                    icon: Ico::Brain,
                    title: "Think",
                    detail: "Summaries, Ask, and Agent",
                    model: think_name,
                    remote: think_location
                        .filter(|url| !lokalbot_core::access::is_loopback_url(url)),
                    ready: status.think_ready
                        || settings.summarizer_backend != SummarizerBackend::BuiltIn,
                    downloading: downloads
                        .get(&settings.built_in_model_id)
                        .map(|d| d.fraction()),
                },
                RoleChoices::Think,
                cx,
            ),
        );
        roles = roles.row(
            "settings.cotypingBuiltInModelID",
            self.role_row(
                RoleRow {
                    id: "settings.cotypingBuiltInModelID",
                    icon: Ico::TextCursorInput,
                    title: "Autocomplete",
                    detail: "Suggestions as you type",
                    model: model_name(&settings.cotyping_built_in_model_id),
                    remote: None,
                    ready: status.autocomplete_ready,
                    downloading: downloads
                        .get(&settings.cotyping_built_in_model_id)
                        .map(|d| d.fraction()),
                },
                RoleChoices::Autocomplete,
                cx,
            ),
        );
        let prepare_data = self.data.clone();
        let any_missing = !(status.transcription_ready && status.think_ready);
        roles = roles.row(
            "settings.prepareModels",
            settings_row::content_row(
                "settings.prepareModels",
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(settings_row::help(
                        if downloads.is_empty() {
                            "Downloads come from the model provider and never include your meeting content."
                        } else {
                            "Downloading models. You can keep using LokalBot meanwhile."
                        },
                        cx,
                    ).flex_1().min_w_0())
                    .child(
                        Button::new("settings-prepare-models")
                            .outline()
                            .small()
                            .icon(Icon::new(Ico::CircleArrowDown))
                            .label("Prepare Selected Models")
                            .disabled(!any_missing || !downloads.is_empty())
                            .on_click(move |_, _, cx| prepare_data.update(cx, |data, cx| data.request(AppEvent::PrepareModels, cx))),
                    ),
                false,
                cx,
            ),
        );

        // Presets.
        let mut presets = Section::new("Presets");
        for preset in Preset::ALL {
            let current = preset.matches(&settings);
            let (speech, think, autocomplete) = preset.models();
            let download: u64 = [speech, think, autocomplete]
                .iter()
                .filter(|id| !status.installed.iter().any(|installed| installed == *id))
                .map(|id| model_bytes(id))
                .sum();
            let data = self.data.clone();
            presets = presets.row(
                if preset == Preset::Balanced {
                    "settings.presetBalanced"
                } else {
                    "settings.presetLightweight"
                },
                settings_row::content_row(
                    if preset == Preset::Balanced {
                        "settings.presetBalanced"
                    } else {
                        "settings.presetLightweight"
                    },
                    h_flex()
                        .gap(px(12.))
                        .items_center()
                        .child(
                            v_flex()
                                .flex_1()
                                .min_w_0()
                                .gap(px(3.))
                                .child(
                                    div()
                                        .text_size(ty::BODY)
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(lb.text)
                                        .child(preset.title()),
                                )
                                .child(
                                    div()
                                        .text_size(ty::SECONDARY)
                                        .text_color(lb.text_secondary)
                                        .child(preset.subtitle()),
                                )
                                .child(
                                    div()
                                        .text_size(ty::CAPTION)
                                        .text_color(lb.text_secondary)
                                        .child(preset.summary()),
                                ),
                        )
                        .when(current, |this| {
                            this.child(
                                div()
                                    .text_size(ty::SECONDARY)
                                    .text_color(lb.accent_text)
                                    .child("Current"),
                            )
                        })
                        .child(
                            Button::new(if preset == Preset::Balanced {
                                "preset-balanced"
                            } else {
                                "preset-lightweight"
                            })
                            .outline()
                            .small()
                            .label("Apply…")
                            .disabled(current)
                            .on_click(move |_, window, cx| {
                                confirm_preset(data.clone(), preset, download, window, cx)
                            }),
                        ),
                    false,
                    cx,
                ),
            );
        }
        presets = presets.row(
            "settings.presetsHelp",
            settings_row::content_row(
                "settings.presetsHelp",
                settings_row::help("Review model changes and download sizes before applying. Existing models are kept.", cx),
                false,
                cx,
            ),
        );

        // Remote servers.
        let page = cx.entity();
        let mut remote = Section::new("Remote Server").row(
            "settings.summarizerBackend",
            settings_row::content_row(
                "settings.summarizerBackend",
                h_flex()
                    .items_center()
                    .child(
                        h_flex()
                            .flex_1()
                            .gap(px(8.))
                            .items_center()
                            .child(ui::icon(Ico::ShieldCheck, px(15.), lb.text_secondary))
                            .child(
                                div()
                                    .text_size(ty::BODY)
                                    .text_color(lb.text_secondary)
                                    .child(
                                        if settings.approved_remote_inference_origins.is_empty() {
                                            "No remote servers approved"
                                        } else {
                                            "Approved remote servers"
                                        },
                                    ),
                            ),
                    )
                    .child(settings_row::link_button(
                        "settings-manage-connections",
                        "Manage Connections…",
                        move |_, cx| {
                            page.update(cx, |page, cx| {
                                page.models.page = ModelsPage::Connections;
                                cx.notify();
                            })
                        },
                        cx,
                    )),
                self.is_focused("settings.summarizerBackend"),
                cx,
            ),
        );
        if settings.approved_remote_inference_origins.is_empty() {
            remote = remote.row(
                "settings.remoteHelp",
                settings_row::content_row(
                    "settings.remoteHelp",
                    settings_row::help("Local models process on this PC. Each remote server requires its own approval before context can leave this PC.", cx),
                    false,
                    cx,
                ),
            );
        }
        for origin in settings.approved_remote_inference_origins.clone() {
            let data = self.data.clone();
            let revoke = origin.clone();
            remote = remote.row(
                "settings.remoteOrigin",
                settings_row::content_row(
                    SharedString::from(format!("settings-origin-{origin}")),
                    v_flex()
                        .gap(px(8.))
                        .child(
                            h_flex()
                                .gap(px(8.))
                                .items_center()
                                .child(ui::icon(Ico::Network, px(15.), lb.attention_text))
                                .child(div().flex_1().text_size(ty::BODY).text_color(lb.text).child(origin.clone()))
                                .child(Button::new(SharedString::from(format!("revoke-{origin}"))).outline().small().label("Revoke…").on_click(
                                    move |_, window, cx| confirm_revoke(data.clone(), revoke.clone(), window, cx),
                                )),
                        )
                        .child(settings_row::help("Includes scheduled daily summaries, which can send activity titles, captured screen text, and meeting evidence to this origin without a prompt each time.", cx)),
                    false,
                    cx,
                ),
            );
        }

        // Storage.
        let models_dir = root.models_dir();
        let open_dir = models_dir.clone();
        let library_root = root.root().to_path_buf();
        let stored = status.installed.len();
        let storage = Section::new("Model Storage").row(
            "settings.modelStorage",
            settings_row::content_row(
                "settings.modelStorage",
                h_flex()
                    .gap(px(12.))
                    .items_center()
                    .child(ui::icon(Ico::HardDrive, px(20.), lb.text_secondary))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(3.))
                            .child(div().text_size(ty::BODY).text_color(lb.text).child(
                                match stored {
                                    0 => "No models downloaded yet".to_string(),
                                    1 => "1 model on this PC".to_string(),
                                    n => format!("{n} models on this PC"),
                                },
                            ))
                            .child(
                                div()
                                    .text_size(ty::SECONDARY)
                                    .text_color(lb.text_secondary)
                                    .truncate()
                                    .child(models_dir.display().to_string()),
                            ),
                    )
                    .child(
                        Button::new("settings-open-models")
                            .outline()
                            .small()
                            .label("Show in Folder")
                            .on_click(move |_, _, cx| {
                                let target = if open_dir.is_dir() {
                                    open_dir.clone()
                                } else {
                                    library_root.clone()
                                };
                                cx.open_with_system(&target);
                            }),
                    ),
                false,
                cx,
            ),
        );

        // Processing budget.
        let budget_data = self.data.clone();
        let budget_index = BUDGETS
            .iter()
            .position(|b| *b == settings.generation_budget_preset)
            .unwrap_or(1);
        let mut budget_column = v_flex()
            .gap(px(8.))
            .child(
                div()
                    .text_size(ty::SECTION)
                    .font_weight(FontWeight::BOLD)
                    .text_color(lb.text)
                    .child("Think processing budget"),
            )
            .child(settings_row::segmented(
                "settings-budget",
                &BUDGETS.map(budget_label),
                budget_index,
                move |index, _, cx| {
                    let budget = BUDGETS[index];
                    budget_data.update(cx, |data, cx| {
                        data.update_settings(|s| s.generation_budget_preset = budget, cx)
                    });
                },
                cx,
            ))
            .child(settings_row::help(
                format!(
                    "Applies to every Think model — built-in and servers. {}",
                    budget_detail(settings.generation_budget_preset)
                ),
                cx,
            ));
        if settings.generation_budget_preset == GenerationBudget::Unlimited {
            budget_column = budget_column.child(
                div().text_size(ty::SECONDARY).text_color(lb.attention_text).child(
                    "Unlimited removes practical caps: a single meeting can process for up to 6 hours. On a paid API this can run up significant token costs; on a local model it can keep this PC busy and hot for a long time.",
                ),
            );
        }
        let budget = Section::new(None).row(
            "settings.generationBudgetPreset",
            settings_row::content_row(
                "settings.generationBudgetPreset",
                budget_column,
                self.is_focused("settings.generationBudgetPreset"),
                cx,
            ),
        );

        let semantic_data = self.data.clone();
        let also = Section::new("Also used by LokalBot").row(
            "settings.semanticSearchEnabled",
            settings_row::row(
                "settings.semanticSearchEnabled",
                "Search by meaning",
                Some(
                    if settings.semantic_search_enabled {
                        "Managed automatically · On this PC"
                    } else {
                        "Off · On this PC"
                    }
                    .into(),
                ),
                settings_row::switch(
                    "settings.semanticSearchEnabled",
                    settings.semantic_search_enabled,
                    move |on, _, cx| {
                        semantic_data.update(cx, |data, cx| {
                            data.update_settings(|s| s.semantic_search_enabled = on, cx)
                        })
                    },
                ),
                self.is_focused("settings.semanticSearchEnabled"),
                cx,
            ),
        );

        vec![summary, roles, presets, remote, storage, budget, also]
    }

    fn role_row(&self, row: RoleRow, choices: RoleChoices, cx: &mut Context<Self>) -> AnyElement {
        let lb = cx.lb().clone();
        let settings = self.data.read(cx).settings.clone();
        let remote = row.remote.clone();
        let tint = if remote.is_some() {
            lb.attention_text
        } else {
            lb.accent_text
        };
        let location: SharedString = match (&remote, row.ready) {
            (Some(url), _) => {
                let host = lokalbot_core::access::origin_of(url).unwrap_or_else(|| url.clone());
                format!("Remote · {host}").into()
            }
            (None, true) => "On this PC · Downloaded".into(),
            (None, false) => "On this PC".into(),
        };
        let status: Option<(SharedString, gpui_kit::Hsla)> =
            match (row.downloading, row.ready, &remote) {
                (Some(fraction), _, _) => Some((
                    format!("Downloading · {}%", (fraction * 100.).round()).into(),
                    lb.text_secondary,
                )),
                (None, false, None) => Some(("Download required".into(), lb.attention_text)),
                _ => None,
            };
        let data = self.data.clone();
        let options: Vec<(SharedString, bool, Box<dyn Fn(&mut Settings)>)> = match choices {
            RoleChoices::Speech => SPEECH_MODELS
                .iter()
                .map(|m| {
                    let id = m.id;
                    (
                        SharedString::from(format!("{} · {}", m.name, size_label(m.bytes))),
                        settings.transcription_model == id,
                        Box::new(move |s: &mut Settings| s.transcription_model = id.into())
                            as Box<dyn Fn(&mut Settings)>,
                    )
                })
                .collect(),
            RoleChoices::Think => {
                let mut items: Vec<(SharedString, bool, Box<dyn Fn(&mut Settings)>)> = TEXT_MODELS
                    .iter()
                    .map(|m| {
                        let id = m.id;
                        (
                            SharedString::from(format!("{} · {}", m.name, size_label(m.bytes))),
                            settings.summarizer_backend == SummarizerBackend::BuiltIn
                                && settings.built_in_model_id == id,
                            Box::new(move |s: &mut Settings| {
                                s.summarizer_backend = SummarizerBackend::BuiltIn;
                                s.built_in_model_id = id.into();
                            }) as Box<dyn Fn(&mut Settings)>,
                        )
                    })
                    .collect();
                items.push((
                    "Ollama server".into(),
                    settings.summarizer_backend == SummarizerBackend::Ollama,
                    Box::new(|s: &mut Settings| s.summarizer_backend = SummarizerBackend::Ollama),
                ));
                items.push((
                    "OpenAI-compatible server".into(),
                    settings.summarizer_backend == SummarizerBackend::OpenAICompatible,
                    Box::new(|s: &mut Settings| {
                        s.summarizer_backend = SummarizerBackend::OpenAICompatible
                    }),
                ));
                items
            }
            RoleChoices::Autocomplete => TEXT_MODELS
                .iter()
                .filter(|m| AUTOCOMPLETE_MODELS.contains(&m.id))
                .map(|m| {
                    let id = m.id;
                    (
                        SharedString::from(format!("{} · {}", m.name, size_label(m.bytes))),
                        settings.cotyping_built_in_model_id == id,
                        Box::new(move |s: &mut Settings| s.cotyping_built_in_model_id = id.into())
                            as Box<dyn Fn(&mut Settings)>,
                    )
                })
                .collect(),
        };
        let options = std::rc::Rc::new(options);
        let change = Button::new(SharedString::from(format!("{}-change", row.id)))
            .outline()
            .small()
            .label("Change…")
            .dropdown_menu(move |mut menu, _, _| {
                for (index, (label, checked, _)) in options.iter().enumerate() {
                    let data = data.clone();
                    let options = options.clone();
                    menu = menu.item(
                        PopupMenuItem::new(label.clone())
                            .checked(*checked)
                            .on_click(move |_, _, cx| {
                                if let Some((_, _, apply)) = options.get(index) {
                                    data.update(cx, |data, cx| {
                                        data.update_settings(|s| apply(s), cx)
                                    });
                                }
                            }),
                    );
                }
                menu
            });

        let reasoning = matches!(choices, RoleChoices::Think).then(|| {
            let data = self.data.clone();
            let current = REASONING
                .iter()
                .find(|(level, _)| *level == settings.think_reasoning_level)
                .map(|(_, l)| *l)
                .unwrap_or("Automatic");
            h_flex()
                .gap(px(6.))
                .items_center()
                .child(
                    div()
                        .text_size(ty::SECONDARY)
                        .text_color(lb.text_secondary)
                        .child("Reasoning"),
                )
                .child(settings_row::picker(
                    "settings-reasoning",
                    current,
                    REASONING
                        .iter()
                        .map(|(level, label)| {
                            (
                                SharedString::from(*label),
                                *level == settings.think_reasoning_level,
                            )
                        })
                        .collect(),
                    move |index, _, cx| {
                        let level = REASONING[index].0;
                        data.update(cx, |data, cx| {
                            data.update_settings(|s| s.think_reasoning_level = level, cx)
                        });
                    },
                ))
        });

        settings_row::content_row(
            row.id,
            h_flex()
                .w_full()
                .gap(px(20.))
                .items_start()
                .py(px(4.))
                .child(
                    h_flex()
                        .w(px(210.))
                        .flex_none()
                        .gap(px(10.))
                        .items_center()
                        .child(
                            div()
                                .w(px(30.))
                                .flex_none()
                                .flex()
                                .justify_center()
                                .child(ui::icon(row.icon, px(20.), tint)),
                        )
                        .child(
                            v_flex()
                                .gap(px(3.))
                                .child(
                                    div()
                                        .text_size(ty::BODY)
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(lb.text)
                                        .child(row.title),
                                )
                                .child(
                                    div()
                                        .text_size(ty::SECONDARY)
                                        .text_color(lb.text_secondary)
                                        .child(row.detail),
                                ),
                        ),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap(px(4.))
                        .child(
                            div()
                                .text_size(ty::BODY)
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(lb.text)
                                .truncate()
                                .child(row.model),
                        )
                        .child(location_label(location, remote.is_some(), cx))
                        .when_some(status, |this, (text, color)| {
                            this.child(
                                h_flex()
                                    .gap(px(5.))
                                    .items_center()
                                    .text_size(ty::SECONDARY)
                                    .text_color(color)
                                    .child(ui::icon(Ico::CircleAlert, px(13.), color))
                                    .child(text),
                            )
                        })
                        .children(reasoning),
                )
                .child(change),
            self.is_focused(row.id),
            cx,
        )
        .into_any_element()
    }

    fn connections(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Vec<Section> {
        let _ = window;
        let lb = cx.lb().clone();
        let settings = self.data.read(cx).settings.clone();
        let page = cx.entity();
        let connection = self.models.connection;
        let is_ollama = connection == SummarizerBackend::Ollama;
        let used = settings.summarizer_backend == connection;
        let (draft_url, _) = self.models.draft(cx);
        let has_changes = self.models.has_changes(&settings, cx);

        let back_page = page.clone();
        let header = v_flex()
            .gap(px(10.))
            .child(
                h_flex().child(
                    Button::new("settings-models-back")
                        .ghost()
                        .small()
                        .icon(Icon::new(Ico::ChevronLeft))
                        .label("Models")
                        .on_click(move |_, _, cx| {
                            back_page.update(cx, |page, cx| {
                                page.models.page = ModelsPage::Active;
                                cx.notify();
                            })
                        }),
                ),
            )
            .child(div().text_size(ty::SECTION).font_weight(FontWeight::BOLD).text_color(lb.text).child("Model connections"))
            .child(settings_row::help("Manage servers and credentials. Choose which provider Think uses under Core Roles.", cx));

        let tab_page = page.clone();
        let tabs = settings_row::segmented(
            "settings-connection",
            &["OpenAI-compatible", "Ollama"],
            usize::from(is_ollama),
            move |index, _, cx| {
                tab_page.update(cx, |page, cx| {
                    page.models.connection = if index == 1 {
                        SummarizerBackend::Ollama
                    } else {
                        SummarizerBackend::OpenAICompatible
                    };
                    page.models.saved = None;
                    cx.notify();
                })
            },
            cx,
        );
        let use_data = self.data.clone();
        let used_row = h_flex()
            .gap(px(8.))
            .items_center()
            .child(ui::icon(
                if used { Ico::CircleCheck } else { Ico::Network },
                px(14.),
                lb.text_secondary,
            ))
            .child(
                div()
                    .text_size(ty::BODY)
                    .text_color(lb.text_secondary)
                    .child(if used {
                        "Used by Think"
                    } else {
                        "Available for Think"
                    }),
            )
            .when(!used, |this| {
                this.child(
                    Button::new("settings-use-connection")
                        .outline()
                        .xsmall()
                        .label("Use for Think")
                        .on_click(move |_, _, cx| {
                            use_data.update(cx, |data, cx| {
                                data.update_settings(|s| s.summarizer_backend = connection, cx)
                            })
                        }),
                )
            });

        let field = |label: &'static str, input: &Entity<InputState>, cx: &App| {
            v_flex()
                .gap(px(5.))
                .child(
                    div()
                        .text_size(ty::BODY)
                        .text_color(cx.lb().text)
                        .child(label),
                )
                .child(Input::new(input))
        };
        let mut form = v_flex().w_full().gap(px(14.)).child(tabs).child(used_row);
        if is_ollama {
            form = form
                .child(field("Server URL", &self.models.ollama_url, cx))
                .child(field("Model ID", &self.models.ollama_model, cx));
        } else {
            let key_data = self.data.clone();
            let key_input = self.models.api_key.clone();
            form = form
                .child(field("Server URL", &self.models.openai_url, cx))
                .child(field("Model ID", &self.models.openai_model, cx))
                .child(
                    v_flex()
                        .gap(px(5.))
                        .child(div().text_size(ty::BODY).text_color(lb.text).child("API key"))
                        .child(
                            h_flex()
                                .gap(px(8.))
                                .child(div().flex_1().child(Input::new(&self.models.api_key).mask_toggle()))
                                .child(Button::new("settings-save-key").outline().small().label("Save key").on_click(move |_, window, cx| {
                                    let key = key_input.read(cx).value().trim().to_string();
                                    if key.is_empty() {
                                        return;
                                    }
                                    key_data.update(cx, |data, cx| data.request(AppEvent::SaveApiKey(key), cx));
                                    key_input.update(cx, |input, cx| input.set_value("", window, cx));
                                })),
                        )
                        .child(settings_row::help("Kept in the system credential store only when you choose Save key. Leave empty for a server that needs no key.", cx)),
                );
        }
        form = form.child(self.endpoint_disclosure(&draft_url, cx));

        let save_page = page.clone();
        form = form.child(
            h_flex()
                .gap(px(12.))
                .items_center()
                .child(
                    Button::new("settings-save-connection")
                        .primary()
                        .small()
                        .label("Save connection")
                        .disabled(!has_changes)
                        .on_click(move |_, _, cx| {
                            save_page.update(cx, |page, cx| page.save_connection(cx))
                        }),
                )
                .when_some(self.models.saved.clone(), |this, saved| {
                    this.child(
                        div()
                            .text_size(ty::SECONDARY)
                            .text_color(lb.text_secondary)
                            .child(saved),
                    )
                }),
        );

        vec![
            Section::new(None).row(
                "settings.connectionsHeader",
                settings_row::content_row("settings.connectionsHeader", header, false, cx),
            ),
            Section::new(None).row(
                "settings.summarizerBackend",
                settings_row::content_row(
                    "settings.summarizerBackend",
                    form.py(px(4.)),
                    self.is_focused("settings.summarizerBackend"),
                    cx,
                ),
            ),
        ]
    }

    /// Loopback, blocked HTTP, or a remote origin with its consent toggle.
    fn endpoint_disclosure(&self, raw_url: &str, cx: &mut Context<Self>) -> Div {
        let lb = cx.lb().clone();
        let settings = self.data.read(cx).settings.clone();
        if raw_url.is_empty() || lokalbot_core::access::is_loopback_url(raw_url) {
            return h_flex()
                .gap(px(8.))
                .items_center()
                .child(ui::icon(Ico::ShieldCheck, px(14.), lb.success_text))
                .child(settings_row::help(
                    "Loopback server: inference context stays on this PC.",
                    cx,
                ));
        }
        let Some(origin) = lokalbot_core::access::origin_of(raw_url) else {
            return h_flex().child(settings_row::help("The server URL is not valid.", cx));
        };
        if !origin.starts_with("https://")
            && !lokalbot_core::access::is_private_network_url(raw_url)
        {
            return h_flex()
                .gap(px(8.))
                .items_start()
                .child(ui::icon(Ico::Lock, px(14.), lb.attention_text))
                .child(
                    div()
                        .text_size(ty::SECONDARY)
                        .text_color(lb.attention_text)
                        .child("Blocked: remote inference must use HTTPS so transcripts and screen text are encrypted in transit."),
                );
        }
        let approved = settings
            .approved_remote_inference_origins
            .iter()
            .any(|o| o.eq_ignore_ascii_case(&origin));
        let host = origin
            .split_once("://")
            .map(|(_, h)| h.to_string())
            .unwrap_or(origin.clone());
        let data = self.data.clone();
        let toggle_origin = origin.clone();
        v_flex()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(8.))
            .bg(lb.status_fill(lb.attention))
            .border_1()
            .border_color(lb.status_stroke(lb.attention))
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(ui::icon(Ico::Network, px(14.), lb.attention_text))
                    .child(div().text_size(ty::BODY).font_weight(FontWeight::SEMIBOLD).text_color(lb.text).child(format!("Remote processing · {host}"))),
            )
            .child(settings_row::help("Think may send meeting transcripts, screen text, Agent context, and dictation requests to this server when you use those features. Scheduled daily summaries also run here and can send activity titles, captured screen text, and meeting evidence without a prompt each time.", cx))
            .child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(div().flex_1().text_size(ty::BODY).text_color(lb.text).child(format!("Allow sending context to {origin}")))
                    .child(settings_row::switch("settings-remote-consent", approved, move |on, _, cx| {
                        let origin = toggle_origin.clone();
                        data.update(cx, |data, cx| {
                            data.update_settings(
                                |s| {
                                    s.approved_remote_inference_origins.retain(|o| !o.eq_ignore_ascii_case(&origin));
                                    if on {
                                        s.approved_remote_inference_origins.push(origin);
                                    }
                                },
                                cx,
                            )
                        })
                    })),
            )
    }

    fn save_connection(&mut self, cx: &mut Context<Self>) {
        let (url, model) = self.models.draft(cx);
        let ollama = self.models.connection == SummarizerBackend::Ollama;
        self.update_settings(
            move |s| {
                if ollama {
                    s.ollama_base_url = url;
                    s.ollama_model = model;
                } else {
                    s.openai_base_url = url;
                    s.openai_model = model;
                }
            },
            cx,
        );
        self.models.saved = Some("Saved".into());
        cx.notify();
    }
}

struct RoleRow {
    id: &'static str,
    icon: Ico,
    title: &'static str,
    detail: &'static str,
    model: String,
    remote: Option<String>,
    ready: bool,
    downloading: Option<f32>,
}

#[derive(Clone, Copy)]
enum RoleChoices {
    Speech,
    Think,
    Autocomplete,
}

/// "On this PC · Downloaded", or a remote origin in an amber capsule.
fn location_label(text: SharedString, remote: bool, cx: &App) -> Div {
    let lb = cx.lb();
    if remote {
        h_flex().child(
            h_flex()
                .gap(px(5.))
                .items_center()
                .px(px(7.))
                .py(px(2.))
                .rounded_full()
                .bg(lb.attention_text.opacity(if lb.dark { 0.16 } else { 0.08 }))
                .text_size(ty::SECONDARY)
                .text_color(lb.attention_text)
                .child(ui::icon(Ico::Network, px(12.), lb.attention_text))
                .child(text),
        )
    } else {
        h_flex()
            .gap(px(5.))
            .items_center()
            .text_size(ty::SECONDARY)
            .text_color(lb.text_secondary)
            .child(ui::icon(Ico::Monitor, px(12.), lb.text_secondary))
            .child(text)
    }
}

fn confirm_preset(
    data: Entity<AppData>,
    preset: Preset,
    download: u64,
    window: &mut Window,
    cx: &mut App,
) {
    let detail = if download > 0 {
        format!(
            "{} Estimated new download: {}. Existing models are kept.",
            preset.summary(),
            size_label(download)
        )
    } else {
        format!("{} Every model is already on this PC.", preset.summary())
    };
    window.open_alert_dialog(cx, move |alert, _, _| {
        let data = data.clone();
        alert
            .title(format!("Use {}?", preset.title()))
            .description(detail.clone())
            .confirm()
            .ok_text(if download > 0 {
                "Download and apply"
            } else {
                "Apply preset"
            })
            .cancel_text("Cancel")
            .on_ok(move |_, _, cx| {
                data.update(cx, |data, cx| {
                    data.update_settings(|s| preset.apply(s), cx);
                    if download > 0 {
                        data.request(AppEvent::PrepareModels, cx);
                    }
                });
                true
            })
    });
}

fn confirm_revoke(data: Entity<AppData>, origin: String, window: &mut Window, cx: &mut App) {
    window.open_alert_dialog(cx, move |alert, _, _| {
        let data = data.clone();
        let origin = origin.clone();
        alert
            .title(format!("Revoke access to {origin}?"))
            .description("This also revokes scheduled processing for this origin. Any model using it will require approval again.")
            .confirm()
            .ok_text("Revoke Access")
            .ok_variant(ButtonVariant::Danger)
            .cancel_text("Cancel")
            .on_ok(move |_, _, cx| {
                let origin = origin.clone();
                data.update(cx, |data, cx| {
                    data.update_settings(|s| s.approved_remote_inference_origins.retain(|o| o != &origin), cx)
                });
                true
            })
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_round_trip() {
        let mut settings = Settings::default();
        Preset::Balanced.apply(&mut settings);
        assert!(Preset::Balanced.matches(&settings));
        assert!(!Preset::Lightweight.matches(&settings));
        assert_eq!(size_label(2_740_937_888), "2.7 GB");
        assert_eq!(size_label(730_895_584), "731 MB");
        assert_eq!(model_name("qwen3.5-4b"), "Qwen3.5 4B");
    }
}
