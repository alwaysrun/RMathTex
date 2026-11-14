//! Application state, two-pane UI and the four output transactions.

use std::time::{Duration, Instant};

use eframe::egui::text::{CCursor, CCursorRange};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, RichText, Sense};

use crate::catalog;
use crate::clipboard;
use crate::config::{self, WindowGeometry};
use crate::convert;
use crate::export;
use crate::linearmath;
use crate::model::{
    self, to_markdown, AppError, CoverageLevel, Delimiter, ExitSavePolicy, ExportStyle,
    ExportTarget, Level, NormalizedSource, Notice, PngBackgroundChoice, RenderKey, Rendered,
    ThemeChoice, AUTO_FLAVOR_FALLBACK, CONFIG_VERSION, DEBOUNCE_MS, DEFAULT_BACKGROUND,
    DEFAULT_DPI, DEFAULT_FONT_SIZE_PT, DIALOG_ACTIONS_GAP, DIALOG_TEXT_SLACK, DPI_CHOICES,
    EDITOR_ROWS, FONT_SIZE_RANGE, HINT_COPY_MARKDOWN, HINT_COPY_OFFICE, HINT_EXIT_POLICY,
    MSG_CANCELLED, MSG_CONFIG_SAVED, MSG_COPY_OK, MSG_DOCX_OMML_FAILED, MSG_EMPTY_SOURCE,
    MSG_EXIT_UNSAVED, MSG_EXPORT_OK, MSG_EXTENDED, MSG_IMPORT_OK, MSG_MATHML_UNAVAILABLE,
    MSG_MD_COPY_OK, MSG_MD_EXPORT_OK, MSG_OMML_UNAVAILABLE, MSG_PLAIN_ONLY, MSG_READY,
    MSG_UI_FONT_MISSING, MSG_UNSUPPORTED, SOURCE_PANEL_FRACTION, TEXT_EDIT_MARGIN_Y,
};
use crate::render::Renderer;

/// All in-memory state of the editor; only the favorites list and the window
/// rectangle are persisted.
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
    favorites: Vec<String>,
    window: Option<WindowGeometry>,
    /// Whether the startup file was readable; a corrupt file must not be
    /// overwritten by the implicit shutdown save.
    config_loaded: bool,
    show_favorites_manager: bool,
    notice: Notice,
    /// Criterion deciding whether closing the window needs confirmation.
    exit_policy: ExitSavePolicy,
    /// Whether the current editor content was exported to a file.
    exported_since_edit: bool,
    /// Whether the current editor content was copied to the clipboard.
    copied_since_edit: bool,
    show_exit_confirm: bool,
    /// Set once the prompt was accepted, so the pending close is not cancelled.
    exit_confirmed: bool,
    /// Whether an IME candidate window is composing into the source editor.
    ///
    /// egui stores the raw preedit (pinyin) in the bound string, so the preview
    /// must not refresh until the candidate is committed.
    ime_composing: bool,
}

impl RMathTexApp {
    /// Builds the app; the embedded math font is the only fallible startup step.
    pub fn new(
        creation: &eframe::CreationContext<'_>,
        startup: Result<config::Config, AppError>,
    ) -> Result<Self, AppError> {
        let renderer = Renderer::new()?;
        let (favorites, window, config_error) = match startup {
            Ok(config) => (config.favorites, config.window, None),
            Err(error) => (catalog::default_favorites(), None, Some(error)),
        };
        let font_ready = install_ui_font(&creation.egui_ctx);
        let config_loaded = config_error.is_none();
        let notice = match (config_error, font_ready) {
            (Some(error), false) => Notice::warn(
                error.code,
                format!("{}；{}", error.message, MSG_UI_FONT_MISSING),
            ),
            (Some(error), true) => Notice::warn(error.code, error.message),
            (None, false) => Notice::warn("E501", MSG_UI_FONT_MISSING),
            (None, true) => Notice::info(MSG_READY),
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
            favorites,
            window,
            config_loaded,
            show_favorites_manager: false,
            notice,
            exit_policy: ExitSavePolicy::default(),
            exported_since_edit: false,
            copied_since_edit: false,
            show_exit_confirm: false,
            exit_confirmed: false,
            ime_composing: false,
        })
    }

    /// Effective display switch: explicit user choice wins over the form hint.
    fn display(&self) -> bool {
        self.display_override
            .unwrap_or_else(|| self.normalized.style_hint.unwrap_or(false))
    }

    /// Applies the effective theme to the whole UI.
    ///
    /// `System` resolves against the Windows light/dark preference reported by
    /// eframe; forcing a theme keeps working even when Windows reports the
    /// opposite mode, because the visuals are re-derived every frame.
    fn sync_theme(&self, ctx: &egui::Context, system_theme: Option<eframe::Theme>) {
        let desired = match self.theme {
            ThemeChoice::System => system_theme.unwrap_or(eframe::Theme::Light).egui_visuals(),
            ThemeChoice::Light => egui::Visuals::light(),
            ThemeChoice::Dark => egui::Visuals::dark(),
        };
        if ctx.style().visuals.dark_mode != desired.dark_mode {
            ctx.set_visuals(desired);
        }
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
        self.mark_edited();
        self.notice = if let Some(error) = model::validate_source(&self.normalized.body) {
            Notice::error(error.code, error.message)
        } else if let Some(warning) = self.normalized.warning {
            Notice::warn("E104", warning)
        } else {
            Notice::info(MSG_READY)
        };
    }

    /// Tracks the IME composition state of the source editor.
    ///
    /// egui inserts the preedit (pinyin) directly into the bound string and keeps
    /// it there while the candidate window is paused, so the state must persist
    /// across frames instead of looking only at the current input events.
    fn track_ime_state(&mut self, ctx: &egui::Context) {
        ctx.input(|input| {
            for event in &input.events {
                match event {
                    egui::Event::Ime(egui::ImeEvent::Preedit(text)) => {
                        self.ime_composing = !text.is_empty();
                    }
                    egui::Event::Ime(egui::ImeEvent::Commit(_) | egui::ImeEvent::Disabled) => {
                        self.ime_composing = false
                    }
                    _ => {}
                }
            }
        });
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

    // ------------------------------------------------------------- exit guard

    /// Forgets the output history, because the editor content just changed and
    /// the previous copy or export no longer covers it.
    fn mark_edited(&mut self) {
        self.exported_since_edit = false;
        self.copied_since_edit = false;
    }

    /// Whether closing the window would drop content that counts as unsaved.
    fn requires_exit_confirmation(&self) -> bool {
        self.exit_policy.requires_confirmation(
            !self.normalized.body.is_empty(),
            self.exported_since_edit,
            self.copied_since_edit,
        )
    }

    /// Handles the menu entry: closes at once, or asks for confirmation first.
    fn request_exit(&mut self, ctx: &egui::Context) {
        if self.requires_exit_confirmation() {
            self.show_exit_confirm = true;
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// Converts an OS close request into a confirmation prompt while the edited
    /// formula has no saved output.
    fn intercept_close(&mut self, ctx: &egui::Context) {
        if self.exit_confirmed || !ctx.input(|input| input.viewport().close_requested()) {
            return;
        }
        if self.requires_exit_confirmation() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.show_exit_confirm = true;
        }
    }

    fn exit_confirmation(&mut self, ctx: &egui::Context) {
        if !self.show_exit_confirm {
            return;
        }
        egui::Window::new("退出确认")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                // Auto-sized windows grow every frame when a right-to-left row
                // fills them, so pin the width to the text instead.
                ui.set_width(exit_dialog_width(ui));
                ui.label(MSG_EXIT_UNSAVED);
                ui.label(RichText::new(HINT_EXIT_POLICY).small().weak());
                ui.add_space(DIALOG_ACTIONS_GAP);
                // Standard confirmation layout: actions right aligned, the
                // primary action rightmost, the safer one next to it.
                //
                // The row height must be bounded explicitly: an unbounded
                // right-to-left layout inside an auto-sized window claims the
                // whole remaining height and turns the prompt into a tall box.
                let row = egui::vec2(ui.available_width(), ui.spacing().interact_size.y);
                ui.allocate_ui_with_layout(
                    row,
                    egui::Layout::right_to_left(egui::Align::Center),
                    |ui| {
                        if ui.button("仍然退出").clicked() {
                            self.show_exit_confirm = false;
                            self.exit_confirmed = true;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                        if ui.button("继续编辑").clicked() {
                            self.show_exit_confirm = false;
                        }
                    },
                );
            });
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
        self.copied_since_edit = true;
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
                self.mark_edited();
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
                self.copied_since_edit = true;
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
                self.exported_since_edit = true;
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
            // Ctrl+Shift+C also reaches egui-winit as `Event::Copy`; the focused
            // editor would replace the Office formats with plain text.
            ctx.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Copy))
            });
        }
        if paste {
            self.import_office();
            // Ctrl+Shift+V also reaches egui-winit as `Event::Paste`; the raw
            // UnicodeMath text would overwrite the imported formula.
            ctx.input_mut(|input| {
                input
                    .events
                    .retain(|event| !matches!(event, egui::Event::Paste(_)));
            });
        }
        if markdown {
            self.copy_markdown();
        }
    }

    // --------------------------------------------------------------------- UI

    fn menu_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        egui::menu::bar(ui, |ui| {
            ui.menu_button("文件", |ui| {
                for target in [
                    ExportTarget::Svg,
                    ExportTarget::Png,
                    ExportTarget::Docx,
                    ExportTarget::Markdown,
                ] {
                    if ui
                        .button(format!("导出 {}", export::target_label(target)))
                        .clicked()
                    {
                        ui.close_menu();
                        self.export_target(target);
                    }
                }
                ui.separator();
                if ui.button("退出").clicked() {
                    ui.close_menu();
                    self.request_exit(ctx);
                }
            });
            ui.menu_button("编辑", |ui| {
                if ui
                    .add(egui::Button::new("复制到 Office").shortcut_text("Ctrl+Shift+C"))
                    .clicked()
                {
                    ui.close_menu();
                    self.copy_office();
                }
                if ui
                    .add(egui::Button::new("从 Office 导入").shortcut_text("Ctrl+Shift+V"))
                    .clicked()
                {
                    ui.close_menu();
                    self.import_office();
                }
                if ui
                    .add(egui::Button::new("复制为 Markdown").shortcut_text("Ctrl+Shift+M"))
                    .clicked()
                {
                    ui.close_menu();
                    self.copy_markdown();
                }
            });
            ui.menu_button("公式", |ui| self.formula_menu(ctx, ui));
            ui.menu_button("设置", |ui| self.settings_menu(ctx, ui));
            ui.menu_button("常用公式", |ui| {
                if ui.button("管理常用公式…").clicked() {
                    ui.close_menu();
                    self.show_favorites_manager = true;
                }
                ui.separator();
                if ui.button("恢复默认").clicked() {
                    ui.close_menu();
                    self.reset_favorites();
                }
            });
        });
    }

    fn formula_menu(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        for category in catalog::FORMULA_CATEGORIES {
            ui.menu_button(category.name, |ui| {
                for entry in category.entries {
                    let text = format!("{}   {}", entry.label, entry.latex);
                    if ui.button(text).clicked() {
                        ui.close_menu();
                        self.insert_snippet(ctx, entry.latex);
                    }
                }
            });
        }
    }

    fn settings_menu(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.menu_button("字号", |ui| {
            ui.add(egui::Slider::new(&mut self.font_size_pt, FONT_SIZE_RANGE).suffix(" pt"));
        });
        ui.menu_button("排版", |ui| {
            let display = self.display();
            if ui.radio(display, "Display").clicked() {
                self.display_override = Some(true);
                ui.close_menu();
            }
            if ui.radio(!display, "Text").clicked() {
                self.display_override = Some(false);
                ui.close_menu();
            }
        });
        ui.menu_button("主题", |ui| {
            for (value, label) in [
                (ThemeChoice::System, "跟随系统"),
                (ThemeChoice::Light, "浅色"),
                (ThemeChoice::Dark, "深色"),
            ] {
                if ui.radio(self.theme == value, label).clicked() {
                    self.theme = value;
                    apply_window_theme(ctx, value);
                    ui.close_menu();
                }
            }
        });
        ui.menu_button("PNG 背景", |ui| {
            for (value, label) in [
                (PngBackgroundChoice::Transparent, "透明"),
                (PngBackgroundChoice::White, "白底"),
            ] {
                if ui.radio(self.png_background == value, label).clicked() {
                    self.png_background = value;
                    ui.close_menu();
                }
            }
        });
        ui.menu_button("PNG DPI", |ui| {
            for dpi in DPI_CHOICES {
                if ui.radio(self.dpi == dpi, dpi.to_string()).clicked() {
                    self.dpi = dpi;
                    ui.close_menu();
                }
            }
        });
        ui.menu_button("Markdown 形态", |ui| {
            if ui
                .radio(self.markdown_flavor.is_none(), "跟随输入")
                .clicked()
            {
                self.markdown_flavor = None;
                ui.close_menu();
            }
            for delimiter in [Delimiter::Inline, Delimiter::Display, Delimiter::Fenced] {
                if ui
                    .radio(
                        self.markdown_flavor == Some(delimiter),
                        delimiter.flavor_name(),
                    )
                    .clicked()
                {
                    self.markdown_flavor = Some(delimiter);
                    ui.close_menu();
                }
            }
            ui.label(RichText::new("行内形态适合单行公式").small().weak());
        });
        ui.menu_button("退出提示", |ui| {
            for (value, label, hint) in [
                (
                    ExitSavePolicy::FileExportOnly,
                    "仅导出文件算已保存",
                    "默认：复制到剪贴板后退出仍会提示",
                ),
                (
                    ExitSavePolicy::AnyOutput,
                    "复制或导出都算已保存",
                    "成功复制或导出后退出不再提示",
                ),
            ] {
                if ui
                    .radio(self.exit_policy == value, label)
                    .on_hover_text(hint)
                    .clicked()
                {
                    self.exit_policy = value;
                    ui.close_menu();
                }
            }
        });
    }

    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .button("复制为Office")
                .on_hover_text(HINT_COPY_OFFICE)
                .clicked()
            {
                self.copy_office();
            }
            if ui
                .button("复制为MD")
                .on_hover_text(HINT_COPY_MARKDOWN)
                .clicked()
            {
                self.copy_markdown();
            }
            ui.separator();
            ui.label("字母");
            let mut selected = None;
            for entry in catalog::greek_palette() {
                if ui.button(entry.label).on_hover_text(entry.latex).clicked() {
                    selected = Some(entry.latex);
                }
            }
            if let Some(latex) = selected {
                self.insert_snippet(ui.ctx(), latex);
            }
        });
    }

    fn favorites_bar(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.label("公式");
            let mut selected = None;
            let mut shown = 0_usize;
            for id in &self.favorites {
                let Some(entry) = catalog::find(id) else {
                    continue;
                };
                // Greek letters are offered on the first toolbar row already.
                if catalog::is_greek(entry.id) {
                    continue;
                }
                shown += 1;
                if ui.button(entry.label).on_hover_text(entry.latex).clicked() {
                    selected = Some(entry.latex);
                }
            }
            if shown == 0 {
                ui.label(
                    RichText::new("未设定，可在「常用公式」菜单中添加")
                        .small()
                        .weak(),
                );
            }
            ui.separator();
            if ui.button("设定…").clicked() {
                self.show_favorites_manager = true;
            }
            if let Some(latex) = selected {
                self.insert_snippet(ctx, latex);
            }
        });
    }

    fn favorites_manager(&mut self, ctx: &egui::Context) {
        if !self.show_favorites_manager {
            return;
        }
        let mut open = self.show_favorites_manager;
        egui::Window::new("常用公式设定")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                ui.label("勾选需要显示在工具栏上的公式。");
                ui.label(
                    RichText::new("希腊字母固定显示在工具栏第一行，不在此处设定")
                        .small()
                        .weak(),
                );
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for category in catalog::FORMULA_CATEGORIES {
                            if category.name == catalog::GREEK_CATEGORY_NAME {
                                continue;
                            }
                            egui::CollapsingHeader::new(category.name).show(ui, |ui| {
                                for entry in category.entries {
                                    let mut checked =
                                        self.favorites.iter().any(|id| id == entry.id);
                                    if ui
                                        .checkbox(
                                            &mut checked,
                                            format!("{}   {}", entry.label, entry.latex),
                                        )
                                        .changed()
                                    {
                                        self.set_favorite(entry.id, checked);
                                        self.persist_favorites();
                                    }
                                }
                            });
                        }
                    });
                if ui.button("恢复默认").clicked() {
                    self.reset_favorites();
                }
            });
        self.show_favorites_manager = open;
    }

    fn set_favorite(&mut self, id: &'static str, enabled: bool) {
        if enabled {
            if !self.favorites.iter().any(|favorite| favorite == id) {
                self.favorites.push(id.to_owned());
            }
        } else {
            self.favorites.retain(|favorite| favorite != id);
        }
        self.favorites = catalog::sanitize(&self.favorites);
    }

    fn reset_favorites(&mut self) {
        self.favorites = catalog::default_favorites();
        self.persist_favorites();
    }

    fn persist_favorites(&mut self) {
        match config::save(&self.config_snapshot()) {
            Ok(path) => {
                self.config_loaded = true;
                let path = path.display().to_string();
                self.notice = Notice::info(fill(MSG_CONFIG_SAVED, &[("path", &path)]));
            }
            Err(error) => self.fail(error),
        }
    }

    /// Snapshot of everything the configuration file owns.
    fn config_snapshot(&self) -> config::Config {
        config::Config {
            version: CONFIG_VERSION,
            favorites: self.favorites.clone(),
            window: self.window,
        }
    }

    /// Mirrors the current viewport rectangle into the persisted geometry.
    fn capture_window(&mut self, ctx: &egui::Context) {
        let (inner, outer, maximized, minimized) = ctx.input(|input| {
            let info = input.viewport();
            (
                info.inner_rect,
                info.outer_rect,
                info.maximized,
                info.minimized,
            )
        });
        // A minimized window reports a degenerate rectangle; keep the last one.
        if minimized == Some(true) {
            return;
        }
        let (Some(inner), Some(outer)) = (inner, outer) else {
            return;
        };
        self.window = Some(WindowGeometry {
            position: [outer.min.x, outer.min.y],
            size: [inner.width(), inner.height()],
            maximized: maximized.unwrap_or(false),
        });
    }

    /// Writes the final snapshot during shutdown, when no status notice can show.
    fn persist_on_exit(&mut self) {
        if !self.config_loaded {
            return;
        }
        if let Err(error) = config::save(&self.config_snapshot()) {
            // The window is already closing, so the log is the only remaining
            // sink; dropping the error would silently lose the saved settings.
            eprintln!("{}: {}", error.code, error.message);
        }
    }

    /// Replaces the editor selection with `snippet`, then moves the caret past it.
    fn insert_snippet(&mut self, ctx: &egui::Context, snippet: &str) {
        let id = Self::editor_id();
        let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
        let (start, end) = match state.cursor.char_range() {
            Some(range) => (
                range.primary.index.min(range.secondary.index),
                range.primary.index.max(range.secondary.index),
            ),
            None => {
                let end = self.source.chars().count();
                (end, end)
            }
        };
        let start_byte = char_to_byte(&self.source, start);
        let end_byte = char_to_byte(&self.source, end);
        self.source.replace_range(start_byte..end_byte, snippet);
        let cursor = start + snippet.chars().count();
        state
            .cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(cursor))));
        state.store(ctx, id);
        ctx.memory_mut(|memory| memory.request_focus(id));
        self.on_source_changed();
    }

    fn editor_id() -> egui::Id {
        egui::Id::new("latex_source_editor")
    }

    /// Turns a Ctrl+V paste of Word linear math into LaTeX.
    ///
    /// The paste event is consumed here, before `TextEdit` sees it, because the
    /// widget would otherwise insert the raw `█(a&=b+c@&=d+e)` text. Any other
    /// paste stays untouched for the standard editing path.
    fn absorb_linear_paste(&mut self, ctx: &egui::Context) {
        let id = Self::editor_id();
        if !ctx.memory(|memory| memory.has_focus(id)) {
            return;
        }
        let mut converted = None;
        ctx.input_mut(|input| {
            let mut kept = Vec::with_capacity(input.events.len());
            for event in std::mem::take(&mut input.events) {
                match &event {
                    egui::Event::Paste(text) if converted.is_none() => {
                        match linearmath::to_latex(text) {
                            Some(latex) => converted = Some(latex),
                            None => kept.push(event),
                        }
                    }
                    _ => kept.push(event),
                }
            }
            input.events = kept;
        });
        if let Some(latex) = converted {
            self.insert_snippet(ctx, &latex);
        }
    }

    fn editor(&mut self, ui: &mut egui::Ui) {
        ui.label("LaTeX 源码");
        self.absorb_linear_paste(ui.ctx());
        egui::ScrollArea::vertical()
            .id_source("source_editor")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // `desired_rows` is the only knob that stretches a multiline
                // TextEdit, so derive it from the panel height instead of
                // leaving the lower half of the panel empty.
                let row_height = ui.text_style_height(&egui::TextStyle::Monospace);
                let usable = (ui.available_height() - TEXT_EDIT_MARGIN_Y).max(0.0);
                let rows = (usable / row_height).floor() as usize;
                let response = ui.add(
                    egui::TextEdit::multiline(&mut self.source)
                        .id(Self::editor_id())
                        .code_editor()
                        .desired_rows(rows.max(EDITOR_ROWS))
                        .desired_width(f32::INFINITY),
                );
                if response.changed() && !self.ime_composing {
                    self.on_source_changed();
                }
            });
    }

    fn preview(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        // A refresh postponed during IME composition must still happen once the
        // candidate is committed, because that commit does not fire `changed()`.
        if !self.ime_composing && self.source != self.normalized.raw {
            self.on_source_changed();
        }
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
                    let painter = ui.painter();
                    painter.add(egui::Shape::Vec(cached.shapes.clone()));
                    paint_fallback_glyphs(painter, cached);
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
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.sync_theme(ctx, frame.info().system_theme);
        self.track_ime_state(ctx);
        self.capture_window(ctx);
        self.intercept_close(ctx);
        self.handle_shortcuts(ctx);
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            self.menu_bar(ctx, ui);
            ui.separator();
            self.toolbar(ui);
            ui.separator();
            self.favorites_bar(ctx, ui);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui));
        // Preview on top, source below: the editor starts at one third of the
        // workspace and the panel handle rebalances the two areas.
        let source_height = ctx.available_rect().height() * SOURCE_PANEL_FRACTION;
        egui::TopBottomPanel::bottom("source")
            .resizable(true)
            .default_height(source_height)
            .show(ctx, |ui| self.editor(ui));
        egui::CentralPanel::default().show(ctx, |ui| self.preview(ctx, ui));
        self.favorites_manager(ctx);
        self.exit_confirmation(ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.persist_on_exit();
    }
}

/// Width of the exit confirmation, measured so the dialog is exactly as wide as
/// its longer text line instead of the runaway width of a right-to-left row.
fn exit_dialog_width(ui: &egui::Ui) -> f32 {
    let message_font = egui::TextStyle::Body.resolve(ui.style());
    let hint_font = egui::TextStyle::Small.resolve(ui.style());
    ui.fonts(|fonts| {
        let message = fonts
            .layout_no_wrap(
                MSG_EXIT_UNSAVED.to_owned(),
                message_font,
                egui::Color32::PLACEHOLDER,
            )
            .size()
            .x;
        let hint = fonts
            .layout_no_wrap(
                HINT_EXIT_POLICY.to_owned(),
                hint_font,
                egui::Color32::PLACEHOLDER,
            )
            .size()
            .x;
        message.max(hint) + DIALOG_TEXT_SLACK
    })
}

/// Paints characters the math face lacks with the UI font.
///
/// The math layout reserved a same-width placeholder for each label, so the
/// glyph only has to be centred on its recorded ink centre.
fn paint_fallback_glyphs(painter: &egui::Painter, rendered: &Rendered) {
    let [red, green, blue] = rendered.key.color;
    let color = egui::Color32::from_rgb(red, green, blue);
    for label in &rendered.labels {
        let font = egui::FontId::proportional(label.size);
        let galley = painter.layout_no_wrap(label.text.to_string(), font, color);
        let ink = galley.mesh_bounds.center();
        // An inkless fallback (zero-width or combining mark) has no bounds to
        // align on, so fall back to the galley box.
        let offset = if ink.is_finite() {
            ink.to_vec2()
        } else {
            galley.size() * 0.5
        };
        painter.galley(label.center - offset, galley, color);
    }
}

/// Registers Windows CJK and symbol fonts as fallbacks of the default families.
///
/// egui bundles Latin-only fonts, so Chinese labels and mathematical symbols
/// (`∓`, `∮`, `⟹`, …) render as tofu boxes unless system fonts are appended to
/// the family fallback chain. Returns `false` when no CJK candidate is readable
/// so the caller can report the degradation; symbol fonts are optional because
/// the CJK fonts already cover the common glyphs.
fn install_ui_font(ctx: &egui::Context) -> bool {
    let mut fonts = egui::FontDefinitions::default();
    let mut fallbacks: Vec<&'static str> = Vec::new();
    let cjk_ready = push_first_readable(&mut fonts, &model::UI_FONT_CANDIDATES, &mut fallbacks);
    push_first_readable(
        &mut fonts,
        &model::UI_SYMBOL_FONT_CANDIDATES,
        &mut fallbacks,
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        let chain = fonts.families.entry(family).or_default();
        chain.extend(fallbacks.iter().map(|&name| name.to_owned()));
    }
    ctx.set_fonts(fonts);
    cjk_ready
}

/// Loads the first readable candidate into `fonts` and records its name in
/// `fallbacks`, preserving the candidate order as fallback priority.
fn push_first_readable(
    fonts: &mut egui::FontDefinitions,
    candidates: &[(&'static str, &'static str)],
    fallbacks: &mut Vec<&'static str>,
) -> bool {
    let Some((name, bytes)) = candidates
        .iter()
        .find_map(|(name, path)| std::fs::read(path).ok().map(|bytes| (*name, bytes)))
    else {
        return false;
    };
    fonts
        .font_data
        .insert(name.to_owned(), egui::FontData::from_owned(bytes));
    fallbacks.push(name);
    true
}

/// Applies the selected theme to the window frame only.
///
/// `ViewportCommand::SetTheme` recolours the native title bar; egui visuals are
/// set separately by [`RMathTexApp::sync_theme`], because the command alone
/// leaves the client area unstyled.
fn apply_window_theme(ctx: &egui::Context, theme: ThemeChoice) {
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

/// Converts a character index into a byte offset, clamped to the text length.
fn char_to_byte(text: &str, index: usize) -> usize {
    text.char_indices()
        .nth(index)
        .map_or(text.len(), |(byte, _)| byte)
}

fn fill(template: &str, pairs: &[(&str, &str)]) -> String {
    let mut text = template.to_owned();
    for (key, value) in pairs {
        text = text.replace(&format!("{{{key}}}"), value);
    }
    text
}
