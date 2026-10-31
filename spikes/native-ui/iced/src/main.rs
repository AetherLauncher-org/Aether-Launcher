//! The iced half of the comparison: the instance list, a rename form with a Chinese text box,
//! and a grid of cubes drawn with wgpu. See `docs/design/native-ui.md`.

use std::time::Instant;

use iced::widget::{
    button, checkbox, column, container, row, scrollable, shader, text, text_input,
};
use iced::{Element, Fill, Subscription, Theme, window};
use native_ui_spike_common::bench::Mode;
use native_ui_spike_common::model::Model;
use native_ui_spike_common::stats;

mod fluent;
mod gpu;

fn main() -> iced::Result {
    let mode = Mode::from_args();
    stats::exit_after_bench("iced", mode);
    iced::application(move || App::new(mode), App::update, App::view)
        .title("Aether 原生界面对比 · iced")
        .subscription(App::subscription)
        .theme(App::theme)
        .window_size((1000.0, 640.0))
        .run()
}

struct App {
    model: Model,
    animate: bool,
    angle: f32,
    last: Option<Instant>,
    dark: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Select(usize),
    Input(String),
    Rename,
    Tick(Instant),
    Animate(bool),
    Dark(bool),
}

impl App {
    fn new(mode: Mode) -> Self {
        Self {
            model: Model::new(),
            animate: mode.animated(),
            angle: 0.6,
            last: None,
            dark: true,
        }
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Select(index) => self.model.select(index),
            Message::Input(value) => self.model.input = value,
            Message::Rename => self.model.rename(),
            Message::Animate(on) => {
                self.animate = on;
                self.last = None;
            }
            Message::Dark(on) => self.dark = on,
            Message::Tick(now) => {
                if let Some(last) = self.last {
                    self.angle += now.duration_since(last).as_secs_f32() * 0.8;
                }
                self.last = Some(now);
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        if self.animate {
            window::frames().map(Message::Tick)
        } else {
            Subscription::none()
        }
    }

    fn theme(&self) -> Theme {
        fluent::theme(self.dark)
    }

    fn view(&self) -> Element<'_, Message> {
        let list = self.model.rows.iter().enumerate().fold(
            column![text("实例").size(20)].spacing(8),
            |list, (index, row_data)| {
                let selected = self.model.selected == Some(index);
                list.push(
                    button(
                        column![
                            text(row_data.name.clone()).size(16),
                            text(format!("{} · {}", row_data.version, row_data.state)).size(12),
                        ]
                        .spacing(2),
                    )
                    .width(Fill)
                    .padding(10)
                    .style(fluent::nav_item(selected))
                    .on_press(Message::Select(index)),
                )
            },
        );

        let form = column![
            text("改名（输入法测试：试试拼音、五笔、日文、候选窗口位置）").size(14),
            row![
                text_input("实例名称", &self.model.input)
                    .on_input(Message::Input)
                    .on_submit(Message::Rename)
                    .style(fluent::input)
                    .padding(10),
                button("重命名")
                    .padding(10)
                    .style(fluent::primary_button)
                    .on_press(Message::Rename),
                button("清空")
                    .padding(10)
                    .style(fluent::secondary_button)
                    .on_press(Message::Input(String::new())),
            ]
            .spacing(8),
            text(self.model.message.clone()).size(13),
            row![
                checkbox(self.animate).label("动画").on_toggle(Message::Animate),
                checkbox(self.dark).label("深色").on_toggle(Message::Dark),
            ]
            .spacing(16),
        ]
        .spacing(8);

        let preview = shader(gpu::CubeView { angle: self.angle }).width(Fill).height(Fill);

        container(row![
            container(scrollable(list)).width(300).padding(12),
            column![
                container(preview).style(fluent::card).padding(1).height(Fill),
                container(form).style(fluent::card).padding(12),
            ]
            .spacing(12)
            .padding(12)
            .width(Fill),
        ])
        .style(fluent::page)
        .into()
    }
}
