use egui::*;

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
pub struct PersistedPreferences {
    pub theme: ThemePreference,
}

impl Default for PersistedPreferences {
    fn default() -> Self {
        Self {
            theme: ThemePreference::System,
        }
    }
}

impl PersistedPreferences {
    fn id() -> Id {
        Id::new("persisted_preference")
    }

    pub fn load(ctx: &Context) -> Option<Self> {
        ctx.data_mut(|d| d.get_persisted(Self::id()))
    }

    pub fn store(self, ctx: &Context) {
        ctx.data_mut(|d| d.insert_persisted(Self::id(), self));
    }
}
