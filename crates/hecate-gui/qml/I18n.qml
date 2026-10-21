pragma Singleton
import QtQuick
import HecateGui

// Text lookups for the pages. Reading StringTable.language inside text() makes every binding that
// calls it depend on the language, so switching updates the whole UI without a restart.
QtObject {
    function text(key) {
        void StringTable.language
        return StringTable.text(key)
    }
}
