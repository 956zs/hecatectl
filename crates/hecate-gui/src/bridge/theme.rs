//! `Theme` QML singleton: the desktop's Material You colours, reloaded when colors.json changes.

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qcolor.h");
        type QColor = cxx_qt_lib::QColor;
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    #[auto_cxx_name]
    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(bool, dark, READ, NOTIFY)]
        #[qproperty(QColor, background, READ, NOTIFY)]
        #[qproperty(QColor, surface, READ, NOTIFY)]
        #[qproperty(QColor, surface_container_low, READ, NOTIFY)]
        #[qproperty(QColor, surface_container, READ, NOTIFY)]
        #[qproperty(QColor, surface_container_high, READ, NOTIFY)]
        #[qproperty(QColor, surface_container_highest, READ, NOTIFY)]
        #[qproperty(QColor, primary, READ, NOTIFY)]
        #[qproperty(QColor, primary_content, READ, NOTIFY)]
        #[qproperty(QColor, primary_container, READ, NOTIFY)]
        #[qproperty(QColor, primary_container_content, READ, NOTIFY)]
        #[qproperty(QColor, secondary_container, READ, NOTIFY)]
        #[qproperty(QColor, secondary_container_content, READ, NOTIFY)]
        #[qproperty(QColor, surface_content, READ, NOTIFY)]
        #[qproperty(QColor, surface_variant_content, READ, NOTIFY)]
        #[qproperty(QColor, outline, READ, NOTIFY)]
        #[qproperty(QColor, outline_variant, READ, NOTIFY)]
        #[qproperty(QColor, error, READ, NOTIFY)]
        #[qproperty(QColor, error_content, READ, NOTIFY)]
        #[qproperty(QColor, error_container, READ, NOTIFY)]
        #[qproperty(QColor, error_container_content, READ, NOTIFY)]
        #[qproperty(QColor, inverse_surface, READ, NOTIFY)]
        #[qproperty(QColor, inverse_surface_content, READ, NOTIFY)]
        #[qproperty(QColor, inverse_primary, READ, NOTIFY)]
        #[qproperty(QString, font_family, READ, CONSTANT)]
        #[qproperty(QString, icon_font_family, READ, CONSTANT)]
        type Theme = super::ThemeRust;
    }

    impl cxx_qt::Threading for Theme {}
    impl cxx_qt::Initialize for Theme {}
}

use super::update;
use crate::config;
use crate::palette::{self, Palette};
use core::pin::Pin;
use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::{QColor, QString};

#[derive(Default)]
pub struct ThemeRust {
    dark: bool,
    background: QColor,
    surface: QColor,
    surface_container_low: QColor,
    surface_container: QColor,
    surface_container_high: QColor,
    surface_container_highest: QColor,
    primary: QColor,
    primary_content: QColor,
    primary_container: QColor,
    primary_container_content: QColor,
    secondary_container: QColor,
    secondary_container_content: QColor,
    surface_content: QColor,
    surface_variant_content: QColor,
    outline: QColor,
    outline_variant: QColor,
    error: QColor,
    error_content: QColor,
    error_container: QColor,
    error_container_content: QColor,
    inverse_surface: QColor,
    inverse_surface_content: QColor,
    inverse_primary: QColor,
    font_family: QString,
    icon_font_family: QString,
}

fn color(palette: &Palette, role: &str) -> QColor {
    let c = palette.get(role).unwrap_or_else(|| panic!("palette role '{role}' missing from the fallback"));
    QColor::from_rgba(c.r.into(), c.g.into(), c.b.into(), c.a.into())
}

/// The colour properties: field, notify signal, colors.json key. The matugen `on_*` roles are
/// named `*_content` here: QML reserves `on<Name>` for signal handlers.
macro_rules! roles {
    ($($field:ident: $notify:ident = $key:literal),* $(,)?) => {
        #[cfg(test)]
        const ROLES: &[&str] = &[$($key),*];

        fn apply(mut theme: Pin<&mut qobject::Theme>, palette: &Palette) {
            update!(theme, dark, dark_changed, palette.is_dark());
            $(update!(theme, $field, $notify, color(palette, $key));)*
        }
    };
}

roles! {
    background: background_changed = "background",
    surface: surface_changed = "surface",
    surface_container_low: surface_container_low_changed = "surface_container_low",
    surface_container: surface_container_changed = "surface_container",
    surface_container_high: surface_container_high_changed = "surface_container_high",
    surface_container_highest: surface_container_highest_changed = "surface_container_highest",
    primary: primary_changed = "primary",
    primary_content: primary_content_changed = "on_primary",
    primary_container: primary_container_changed = "primary_container",
    primary_container_content: primary_container_content_changed = "on_primary_container",
    secondary_container: secondary_container_changed = "secondary_container",
    secondary_container_content: secondary_container_content_changed = "on_secondary_container",
    surface_content: surface_content_changed = "on_surface",
    surface_variant_content: surface_variant_content_changed = "on_surface_variant",
    outline: outline_changed = "outline",
    outline_variant: outline_variant_changed = "outline_variant",
    error: error_changed = "error",
    error_content: error_content_changed = "on_error",
    error_container: error_container_changed = "error_container",
    error_container_content: error_container_content_changed = "on_error_container",
    inverse_surface: inverse_surface_changed = "inverse_surface",
    inverse_surface_content: inverse_surface_content_changed = "inverse_on_surface",
    inverse_primary: inverse_primary_changed = "inverse_primary",
}

impl cxx_qt::Initialize for qobject::Theme {
    fn initialize(mut self: Pin<&mut Self>) {
        let cfg = config::get();
        self.as_mut().rust_mut().font_family = QString::from(&cfg.font_family);
        self.as_mut().rust_mut().icon_font_family = QString::from(&cfg.icon_font_family);

        // The first load runs here so the window never shows the fallback colours first.
        let path = cfg.theme_colors_path();
        let (palette, stamp) = palette::load_stamped(path.as_deref());
        apply(self.as_mut(), &palette);

        let qt = self.qt_thread();
        let spawned = std::thread::Builder::new().name("hecate-theme".into()).spawn(move || {
            palette::watch(path.as_deref(), stamp, cfg.theme_poll, |palette| {
                qt.queue(move |theme| apply(theme, &palette)).is_ok()
            })
        });
        if let Err(e) = spawned {
            eprintln!("hecate-gui: cannot start the theme watcher: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_property_role_is_in_the_fallback() {
        let fallback = Palette::fallback();
        for role in ROLES {
            assert!(fallback.get(role).is_some(), "{role}");
        }
    }
}
