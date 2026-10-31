//! The egui half of the comparison: the same instance list, rename form and cube grid as the
//! iced spike. See `docs/design/native-ui.md`.

use std::sync::Arc;

use eframe::egui;
use native_ui_spike_common::bench::Mode;
use native_ui_spike_common::model::Model;
use native_ui_spike_common::stats;

mod gpu;

fn main() -> eframe::Result {
    let mode = Mode::from_args();
    stats::exit_after_bench("egui", mode);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1000.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Aether 原生界面对比 · egui",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, mode)))),
    )
}

/// egui draws text with the fonts it is given and does not look at the system's, so a Chinese
/// font has to be found and added by hand. This is the part iced does not need.
fn install_cjk_font(ctx: &egui::Context) -> Option<&'static str> {
    const CANDIDATES: &[&str] = &[
        "C:\\Windows\\Fonts\\msyh.ttc",
        "C:\\Windows\\Fonts\\simhei.ttf",
        "C:\\Windows\\Fonts\\simsun.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    ];
    for path in CANDIDATES {
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let mut fonts = egui::FontDefinitions::default();
        fonts
            .font_data
            .insert("cjk".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
            if let Some(list) = fonts.families.get_mut(&family) {
                list.push("cjk".to_owned());
            }
        }
        ctx.set_fonts(fonts);
        return Some(path);
    }
    None
}

struct App {
    model: Model,
    animate: bool,
    dark: bool,
    angle: f32,
    last: Option<std::time::Instant>,
    scene: Option<gpu::Scene>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, mode: Mode) -> Self {
        if install_cjk_font(&cc.egui_ctx).is_none() {
            eprintln!("no Chinese font found: Chinese text will show as boxes");
        }
        let scene = cc
            .wgpu_render_state
            .as_ref()
            .map(|state| gpu::Scene::new(&state.device));
        Self {
            model: Model::new(),
            animate: mode.animated(),
            dark: true,
            angle: 0.6,
            last: None,
            scene,
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        stats::record_frame();
        let ctx = ui.ctx().clone();
        ctx.set_visuals(if self.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });

        let now = std::time::Instant::now();
        if self.animate {
            if let Some(last) = self.last {
                self.angle += now.duration_since(last).as_secs_f32() * 0.8;
            }
            self.last = Some(now);
            ctx.request_repaint();
        } else {
            self.last = None;
        }

        egui::Panel::left("instances")
            .exact_size(300.0)
            .show(ui, |ui| {
                ui.heading("实例");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let mut clicked = None;
                    for (index, row) in self.model.rows.iter().enumerate() {
                        let selected = self.model.selected == Some(index);
                        let label = format!("{}\n{} · {}", row.name, row.version, row.state);
                        if ui
                            .add_sized(
                                [ui.available_width(), 48.0],
                                egui::Button::selectable(selected, label),
                            )
                            .clicked()
                        {
                            clicked = Some(index);
                        }
                    }
                    if let Some(index) = clicked {
                        self.model.select(index);
                    }
                });
            });

        egui::CentralPanel::default().show(ui, |ui| {
            let form_height = 150.0;
            let size = egui::vec2(
                ui.available_width(),
                (ui.available_height() - form_height).max(120.0),
            );
            if let (Some(scene), Some(state)) = (self.scene.as_mut(), frame.wgpu_render_state()) {
                let scale = ctx.pixels_per_point();
                let pixels = (
                    (size.x * scale).round().max(1.0) as u32,
                    (size.y * scale).round().max(1.0) as u32,
                );
                let texture = scene.render(state, pixels, self.angle);
                ui.add(egui::Image::new((texture, size)));
            }

            ui.separator();
            ui.label("改名（输入法测试：试试拼音、五笔、日文、候选窗口位置）");
            ui.horizontal(|ui| {
                let edit = ui.add(
                    egui::TextEdit::singleline(&mut self.model.input)
                        .hint_text("实例名称")
                        .desired_width((ui.available_width() - 90.0).max(100.0)),
                );
                let entered = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.button("重命名").clicked() || entered {
                    self.model.rename();
                }
            });
            ui.label(&self.model.message);
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.animate, "动画");
                ui.checkbox(&mut self.dark, "深色");
            });
        });
    }
}
