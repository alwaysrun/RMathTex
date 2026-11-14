//! Application state, two-pane UI and the four output transactions.

use std::time::{Duration, Instant};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText, Sense};

use crate::clipboard;
use crate::convert;
use crate::export;
use crate::model::{
    self, to_markdown, AppError, CoverageLevel, Delimiter, ExportStyle, ExportTarget, Level,
    NormalizedSource, Notice, PngBackgroundChoice, RenderKey, Rendered, ThemeChoice,
    AUTO_FLAVOR_FALLBACK, DEBOUNCE_MS, DEFAULT_BACKGROUND, DEFAULT_DPI, DEFAULT_FONT_SIZE_PT,
    DPI_CHOICES, EDITOR_ROWS, FONT_SIZE_RANGE, MSG_CANCELLED, MSG_COPY_OK, MSG_DOCX_OMML_FAILED,
    MSG_EMPTY_SOURCE, MSG_EXPORT_OK, MSG_EXTENDED, MSG_IMPORT_OK, MSG_MATHML_UNAVAILABLE,
    MSG_MD_COPY_OK, MSG_MD_EXPORT_OK, MSG_OMML_UNAVAILABLE, MSG_PLAIN_ONLY, MSG_READY,
    MSG_UI_FONT_MISSING, MSG_UNSUPPORTED,
};
use crate::render::Renderer;

/// All in-memory state of the editor; nothing is persisted.
pub struct RMathTexApp {
    source: String,
    normalized: NormalizedSource,
    renderer: Renderer,
    cache: Option<Rendered>,
    last_edit: Option<Instant>,
    font_size_pt: i64,
    display_override: Option<bool>,
    theme: ThemeChoice,
    dpi: i64,
    png_background: PngBackgroundChoice,
    markdown_flavor: Option<Delimiter>,
    notice: Notice,
}

impl RMathTexApp {
    /// Builds the app; the embedded math font is the only fallible startup step.
    pub fn new(creation: &eframe::CreationContext<'_>) -> Result<Self, AppError> {
        let renderer = Renderer::new()?;
        let notice = if install_ui_font(&creation.egui_ctx) {
            Notice::info(MSG_READY)
        } else {
            Notice::warn("E501", MSG_UI_FONT_MISSING)
        };
        Ok(Self {
            source: String::new(),
            normalized: model::normalize_source(""),
            renderer,
            cache: None,
            last_edit: None,
            font_size_pt: DEFAULT_FONT_SIZE_PT,
            display_override: None,
            theme: ThemeChoice::System,
            dpi: DEFAULT_DPI,
            png_background: DEFAULT_BACKGROUND,
            markdown_flavor: None,
            notice,
        })
    }

    /// Effective display switch: explicit user choice wins over the form hint.
    fn display(&self) -> bool {
        self.display_override
            .unwrap_or_else(|| self.normalized.style_hint.unwrap_or(false))
    }

    fn render_key(&self, ctx: &egui::Context) -> RenderKey {
        let color = ctx.style().visuals.text_color().to_array();
        RenderKey::new(
            &self.normalized.body,
            self.font_size_pt,
            self.display(),
            ctx.pixels_per_point(),
            [color[0], color[1], color[2]],
        )
    }

    fn export_style(&self) -> ExportStyle {
        ExportStyle {
            display: self.display(),
            dpi: self.dpi,
            background: self.png_background,
            ..ExportStyle::default()
        }
    }

    fn effective_flavor(&self) -> Delimiter {
        match self.markdown_flavor {
            Some(flavor) => flavor,
            None if self.normalized.delimiter == Delimiter::None => AUTO_FLAVOR_FALLBACK,
            None => self.normalized.delimiter,
        }
    }

    /// Pure read: wraps the normalized body for all three Markdown paths.
    fn markdown_output(&self) -> String {
        to_markdown(&self.normalized.body, self.effective_flavor())
    }

    fn on_source_changed(&mut self) {
        self.normalized = model::normalize_source(&self.source);
        self.display_override = None;
        self.last_edit = Some(Instant::now());
        self.notice = if let Some(error) = model::validate_source(&self.normalized.body) {
            Notice::error(error.code, error.message)
        } else if let Some(warning) = self.normalized.warning {
            Notice::warn("E104", warning)
        } else {
            Notice::info(MSG_READY)
        };
    }

    fn precheck(&self) -> Option<AppError> {
        if self.normalized.body.is_empty() {
            return Some(AppError::new("E103", MSG_EMPTY_SOURCE));
        }
        model::validate_source(&self.normalized.body)
    }

    fn fail(&mut self, error: AppError) {
        self.notice = Notice::error(error.code, error.message);
    }

    // ------------------------------------------------------------- transactions

    fn copy_office(&mut self) {
        if let Some(error) = self.precheck() {
            self.fail(error);
            return;
        }
        let body = self.normalized.body.clone();
        let display = self.display();
        let coverage = convert::check_capability(&body);
        if coverage.level == CoverageLevel::Unsupported {
            let text = fill(
                MSG_UNSUPPORTED,
                &[("commands", &coverage.blocked.join(", "))],
            );
            self.notice = Notice::error("E302", text);
            return;
        }
        let mathml = convert::to_mathml(&body, display).ok();
        let omml = convert::to_omml(&body).ok();
        let written = match clipboard::write_office(&body, mathml.as_deref(), omml.as_deref()) {
            Ok(written) => written,
            Err(error) => return self.fail(error),
        };
        let base = fill(MSG_COPY_OK, &[("formats", &written.join(" + "))]);
        let warning = if coverage.level == CoverageLevel::Extended {
            Some(("E305", MSG_EXTENDED))
        } else if mathml.is_none() && omml.is_none() {
            Some(("E303", MSG_PLAIN_ONLY))
        } else if mathml.is_none() {
            Some(("E301", MSG_MATHML_UNAVAILABLE))
        } else if omml.is_none() {
            Some(("E303", MSG_OMML_UNAVAILABLE))
        } else {
            None
        };
        self.notice = match warning {
            Some((code, text)) => Notice::warn(code, format!("{base}；{text}")),
            None => Notice::info(base),
        };
    }

    fn import_office(&mut self) {
        match clipboard::read_office() {
            Ok(body) => {
                self.source = body;
                self.normalized = model::normalize_source(&self.source);
                self.display_override = None;
                self.last_edit = Some(Instant::now());
                self.notice = Notice::info(MSG_IMPORT_OK);
            }
            Err(error) => self.fail(error),
        }
    }

    fn copy_markdown(&mut self) {
        if let Some(error) = self.precheck() {
            self.fail(error);
            return;
        }
        let text = self.markdown_output();
        let flavor = self.effective_flavor();
        match clipboard::write_text(&text) {
            Ok(()) => {
                let notice = fill(MSG_MD_COPY_OK, &[("flavor", flavor.flavor_name())]);
                self.notice = Notice::info(notice);
            }
            Err(error) => self.fail(error),
        }
    }

    fn export_target(&mut self, target: ExportTarget) {
        if let Some(error) = self.precheck() {
            self.fail(error);
            return;
        }
        let coverage = convert::check_capability(&self.normalized.body);
        if coverage.level == CoverageLevel::Unsupported {
            let text = fill(
                MSG_UNSUPPORTED,
                &[("commands", &coverage.blocked.join(", "))],
            );
            self.notice = Notice::error("E302", text);
            return;
        }
        let style = self.export_style();
        let bytes = match self.build_export(target, &style) {
            Ok(bytes) => bytes,
            Err(error) => return self.fail(error),
        };
        match export::save(target, &bytes) {
            Ok(Some(path)) => {
                let path = path.display().to_string();
                self.notice = if target == ExportTarget::Markdown {
                    Notice::info(fill(
                        MSG_MD_EXPORT_OK,
                        &[
                            ("flavor", self.effective_flavor().flavor_name()),
                            ("path", &path),
                        ],
                    ))
                } else {
                    Notice::info(fill(
                        MSG_EXPORT_OK,
                        &[
                            ("target", export::target_label(target)),
                            ("path", &path),
                            ("dpi", &style.dpi.to_string()),
                            ("background", background_label(style.background)),
                        ],
                    ))
                };
            }
            Ok(None) => {
                self.notice = Notice {
                    level: Level::Info,
                    text: MSG_CANCELLED.to_owned(),
                    code: Some("E604"),
                };
            }
            Err(error) => self.fail(error),
        }
    }

    fn build_export(&self, target: ExportTarget, style: &ExportStyle) -> Result<Vec<u8>, AppError> {
        let body = &self.normalized.body;
        match target {
            ExportTarget::Svg => Ok(export::svg(body, style, self.renderer.font())?.into_bytes()),
            ExportTarget::Png => export::png(body, style, self.renderer.font()),
            ExportTarget::Docx => {
                let omml = convert::to_omml(body)
                    .map_err(|error| AppError::caused("E602", MSG_DOCX_OMML_FAILED, error))?;
                export::docx(&omml, style.display)
            }
            ExportTarget::Markdown => Ok(self.markdown_output().into_bytes()),
        }
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        let copy = KeyboardShortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::C);
        let paste = KeyboardShortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::V);
        let markdown = KeyboardShortcut::new(Modifiers::CTRL | Modifiers::SHIFT, Key::M);
        let (copy, paste, markdown) = ctx.input_mut(|input| {
            (
                input.consume_shortcut(&copy),
                input.consume_shortcut(&paste),
                input.consume_shortcut(&markdown),
            )
        });
        if copy {
            self.copy_office();
        }
        if paste {
            self.import_office();
        }
        if markdown {
            self.copy_markdown();
        }
    }

    // --------------------------------------------------------------------- UI

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label("字号");
            ui.add(egui::Slider::new(&mut self.font_size_pt, FONT_SIZE_RANGE).suffix(" pt"));
            ui.separator();
            ui.label("排版");
            let display = self.display();
            if ui.radio(display, "Display").clicked() {
                self.display_override = Some(true);
            }
            if ui.radio(!display, "Text").clicked() {
                self.display_override = Some(false);
            }
            ui.separator();
            ui.label("主题");
            let mut theme = self.theme;
            ui.radio_value(&mut theme, ThemeChoice::System, "跟随系统");
            ui.radio_value(&mut theme, ThemeChoice::Light, "亮");
            ui.radio_value(&mut theme, ThemeChoice::Dark, "暗");
            if theme != self.theme {
                self.theme = theme;
                apply_theme(ui.ctx(), theme);
            }
            ui.separator();
            ui.label("PNG 背景");
            ui.radio_value(
                &mut self.png_background,
                PngBackgroundChoice::Transparent,
                "透明",
            );
            ui.radio_value(&mut self.png_background, PngBackgroundChoice::White, "白底");
        });
        ui.horizontal(|ui| {
            ui.label("DPI");
            egui::ComboBox::from_id_source("dpi")
                .selected_text(self.dpi.to_string())
                .show_ui(ui, |ui| {
                    for dpi in DPI_CHOICES {
                        ui.selectable_value(&mut self.dpi, dpi, dpi.to_string());
                    }
                });
            ui.separator();
            ui.label("Markdown 形态");
            egui::ComboBox::from_id_source("flavor")
                .selected_text(flavor_label(self.markdown_flavor))
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.markdown_flavor, None, "跟随输入");
                    for delimiter in [
                        Delimiter::None,
                        Delimiter::Inline,
                        Delimiter::Display,
                        Delimiter::MacroInline,
                        Delimiter::MacroDisplay,
                        Delimiter::Fenced,
                    ] {
                        ui.selectable_value(
                            &mut self.markdown_flavor,
                            Some(delimiter),
                            delimiter.flavor_name(),
                        );
                    }
                });
            ui.label(RichText::new("（行内形态适合单行公式）").small().weak());
        });
        ui.horizontal(|ui| {
            if ui.button("复制到 Office").clicked() {
                self.copy_office();
            }
            if ui.button("从 Office 导入").clicked() {
                self.import_office();
            }
            if ui.button("复制为 Markdown").clicked() {
                self.copy_markdown();
            }
            ui.separator();
            for target in [
                ExportTarget::Svg,
                ExportTarget::Png,
                ExportTarget::Docx,
                ExportTarget::Markdown,
            ] {
                let label = format!("导出 {}", export::target_label(target));
                if ui.button(label).clicked() {
                    self.export_target(target);
                }
            }
        });
        ui.add_space(2.0);
    }

    fn editor(&mut self, ui: &mut egui::Ui, height: f32) {
        ui.label("LaTeX 源码");
        egui::ScrollArea::vertical()
            .id_source("source_editor")
            .max_height(height)
            .show(ui, |ui| {
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.source)
                        .code_editor()
                        .desired_rows(EDITOR_ROWS)
                        .desired_width(f32::INFINITY),
                );
                if response.changed() {
                    self.on_source_changed();
                }
            });
    }

    fn preview(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.label("预览");
        ui.separator();
        if self.normalized.body.is_empty()
            || model::validate_source(&self.normalized.body).is_some()
        {
            self.cache = None;
            return;
        }
        egui::ScrollArea::both()
            .id_source("preview_scroll")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let origin = ui.available_rect_before_wrap().min;
                let key = self.render_key(ctx);
                let pending = self
                    .last_edit
                    .is_some_and(|edited| edited.elapsed() < Duration::from_millis(DEBOUNCE_MS));
                let stale = match &self.cache {
                    None => true,
                    Some(cached) => cached.key != key || cached.rect.min != origin,
                };
                if pending {
                    ctx.request_repaint_after(Duration::from_millis(DEBOUNCE_MS));
                } else if stale {
                    self.last_edit = None;
                    match self.renderer.render(&self.normalized.body, key, origin) {
                        Ok(rendered) => self.cache = Some(rendered),
                        Err(error) => self.fail(error),
                    }
                }
                if let Some(cached) = &self.cache {
                    let size = cached.rect.size().max(egui::vec2(0.0, 0.0));
                    ui.allocate_exact_size(size, Sense::hover());
                    ui.painter().add(egui::Shape::Vec(cached.shapes.clone()));
                }
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let color = match self.notice.level {
            Level::Info => ui.visuals().text_color(),
            Level::Warn => egui::Color32::from_rgb(214, 137, 16),
            Level::Error => egui::Color32::from_rgb(200, 55, 55),
        };
        let text = match self.notice.code {
            Some(code) => format!("{} 〔{code}〕", self.notice.text),
            None => self.notice.text.clone(),
        };
        ui.add_space(2.0);
        ui.label(RichText::new(text).color(color));
        ui.add_space(2.0);
    }
}

impl eframe::App for RMathTexApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ctx);
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| self.toolbar(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui));
        egui::CentralPanel::default().show(ctx, |ui| {
            let editor_height = (ui.available_height() * 0.4).max(80.0);
            self.editor(ui, editor_height);
            ui.separator();
            self.preview(ctx, ui);
        });
    }
}

/// Registers the first readable Windows CJK font as a fallback of the default families.
///
/// egui bundles Latin-only fonts, so every Chinese label renders as a tofu box
/// unless a system font is appended to the family fallback chain. Returns `false`
/// when no candidate file is readable so the caller can report the degradation.
fn install_ui_font(ctx: &egui::Context) -> bool {
    let Some((name, bytes)) = model::UI_FONT_CANDIDATES
        .iter()
        .find_map(|(name, path)| std::fs::read(path).ok().map(|bytes| (*name, bytes)))
    else {
        return false;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts
        .font_data
        .insert(name.to_owned(), egui::FontData::from_owned(bytes));
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .entry(family)
            .or_default()
            .push(name.to_owned());
    }
    ctx.set_fonts(fonts);
    true
}

fn apply_theme(ctx: &egui::Context, theme: ThemeChoice) {
    let system = match theme {
        ThemeChoice::System => egui::SystemTheme::SystemDefault,
        ThemeChoice::Light => egui::SystemTheme::Light,
        ThemeChoice::Dark => egui::SystemTheme::Dark,
    };
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(system));
}

fn background_label(background: PngBackgroundChoice) -> &'static str {
    match background {
        PngBackgroundChoice::Transparent => "透明底",
        PngBackgroundChoice::White => "白底",
    }
}

fn flavor_label(flavor: Option<Delimiter>) -> &'static str {
    flavor.map_or("跟随输入", Delimiter::flavor_name)
}

fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut text = template.to_owned();
    for (key, value) in pairs {
        text = text.replace(&format!("{{{key}}}"), value);
    }
    text
}
