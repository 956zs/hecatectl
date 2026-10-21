//! QObjects registered in the `HecateGui` QML module (see build.rs).

pub mod device;
pub mod i18n;
pub mod macros;
pub mod theme;

/// Sets a read-only property field and emits its notify signal when the value changed, like the
/// setter cxx-qt generates for writable properties.
macro_rules! update {
    ($obj:ident, $field:ident, $notify:ident, $value:expr) => {{
        let value = $value;
        if $obj.$field != value {
            $obj.as_mut().rust_mut().$field = value;
            $obj.as_mut().$notify();
        }
    }};
}
pub(crate) use update;
