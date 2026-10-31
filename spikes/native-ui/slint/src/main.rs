//! The Slint half of the comparison: the same instance list, rename form and cube grid as the
//! iced and egui spikes. See `docs/design/native-ui.md`.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use native_ui_spike_common::bench::Mode;
use native_ui_spike_common::model::Model;
use native_ui_spike_common::stats;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

mod gpu;

slint::include_modules!();

/// Puts the model's rows, selection and message on the window.
fn show(window: &MainWindow, model: &Model) {
    let rows: Vec<Row> = model
        .rows
        .iter()
        .map(|row| Row {
            name: SharedString::from(row.name.as_str()),
            detail: SharedString::from(format!("{} · {}", row.version, row.state)),
        })
        .collect();
    window.set_rows(ModelRc::new(VecModel::from(rows)));
    window.set_selected(model.selected.map_or(-1, |index| index as i32));
    window.set_input(SharedString::from(model.input.as_str()));
    window.set_message(SharedString::from(model.message.as_str()));
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = Mode::from_args();
    stats::exit_after_bench("slint", mode);

    slint::BackendSelector::new()
        .require_wgpu_30(slint::wgpu_30::WGPUConfiguration::default())
        .select()?;

    let window = MainWindow::new()?;
    window.set_animate(mode.animated());

    let model = Rc::new(RefCell::new(Model::new()));
    show(&window, &model.borrow());

    window.on_select({
        let window = window.as_weak();
        let model = model.clone();
        move |index| {
            let Some(window) = window.upgrade() else {
                return;
            };
            model.borrow_mut().select(index as usize);
            show(&window, &model.borrow());
        }
    });
    window.on_rename({
        let window = window.as_weak();
        let model = model.clone();
        move || {
            let Some(window) = window.upgrade() else {
                return;
            };
            let mut model = model.borrow_mut();
            model.input = window.get_input().to_string();
            model.rename();
            show(&window, &model);
        }
    });

    // The cube: created when Slint's wgpu device is ready, drawn before each frame.
    let scene: RefCell<Option<gpu::Scene>> = RefCell::new(None);
    let started = Instant::now();
    let mut angle = 0.6_f32;
    let mut last = started;
    window.window().set_rendering_notifier({
        let window = window.as_weak();
        move |state, graphics_api| {
            let Some(window) = window.upgrade() else {
                return;
            };
            let slint::GraphicsAPI::WGPU30 { device, queue, .. } = graphics_api else {
                return;
            };
            match state {
                slint::RenderingState::RenderingSetup => {
                    let built = gpu::Scene::new(device);
                    if let Ok(image) = slint::Image::try_from(built.texture()) {
                        window.set_cube(image);
                    }
                    built.render(device, queue, angle);
                    *scene.borrow_mut() = Some(built);
                }
                slint::RenderingState::BeforeRendering => {
                    stats::record_frame();
                    let now = Instant::now();
                    if window.get_animate() {
                        angle += now.duration_since(last).as_secs_f32() * 0.8;
                        if let Some(scene) = scene.borrow().as_ref() {
                            scene.render(device, queue, angle);
                        }
                        // keep drawing while it spins; when it stops, so does the redraw
                        window.window().request_redraw();
                    }
                    last = now;
                }
                slint::RenderingState::RenderingTeardown => {
                    *scene.borrow_mut() = None;
                }
                _ => {}
            }
        }
    })?;

    window.run()?;
    Ok(())
}
