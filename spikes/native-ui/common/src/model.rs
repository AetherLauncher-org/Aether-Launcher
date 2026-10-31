//! The state and the actions of the spike window, independent of any toolkit: an instance
//! list read through `aether-app`, a selection, and a rename form (the Chinese-input test).

use aether_app::instances;
use aether_app::manage;

use crate::host::SpikeHost;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub id: String,
    pub name: String,
    pub version: String,
    pub state: &'static str,
}

pub struct Model {
    host: SpikeHost,
    pub rows: Vec<Row>,
    pub selected: Option<usize>,
    /// The text box of the rename form.
    pub input: String,
    /// What the last action said.
    pub message: String,
}

impl Model {
    #[must_use]
    pub fn new() -> Self {
        let mut model = Self {
            host: SpikeHost::open(),
            rows: Vec::new(),
            selected: None,
            input: String::new(),
            message: "选一个实例，在下面的输入框里改名。".to_owned(),
        };
        model.refresh();
        model.select(0);
        model
    }

    pub fn refresh(&mut self) {
        match instances::list(&self.host) {
            Ok(listing) => {
                self.rows = listing
                    .instances()
                    .iter()
                    .map(|summary| Row {
                        id: summary.id().to_owned(),
                        name: summary.name().to_owned(),
                        version: summary.version().to_owned(),
                        state: summary.state(),
                    })
                    .collect();
                self.rows.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            Err(error) => self.message = error,
        }
        if self.selected.is_some_and(|index| index >= self.rows.len()) {
            self.selected = None;
        }
    }

    pub fn select(&mut self, index: usize) {
        if let Some(row) = self.rows.get(index) {
            self.selected = Some(index);
            self.input.clone_from(&row.name);
        }
    }

    /// Renames the selected instance to the text in the form.
    pub fn rename(&mut self) {
        let Some(row) = self.selected.and_then(|index| self.rows.get(index)).cloned() else {
            self.message = "还没有选实例。".to_owned();
            return;
        };
        match manage::rename(&self.host, &row.id, &self.input) {
            Ok(renamed) => {
                self.message = format!("已改名为「{}」", renamed.name());
                self.refresh();
                self.selected = self.rows.iter().position(|r| r.id == row.id);
            }
            Err(error) => self.message = format!("没改成：{error}"),
        }
    }
}

impl Default for Model {
    fn default() -> Self {
        Self::new()
    }
}
