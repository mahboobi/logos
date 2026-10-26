// Icons.qml — SVG path data for the UI's icons, as a QML singleton.
// Register with a qmldir line:  singleton Icons 1.0 Icons.qml
//
// Paths are stored rather than .svg files on purpose. A Shape draws them in
// whatever colour the caller asks for, so one glyph serves light chrome, dark
// chrome and the disabled state without three recoloured assets; it stays
// crisp at any scale; and it needs nothing added to the module package, which
// matters because QML UI modules cannot read files outside their own
// directory (CLAUDE.md, platform constraint 1).
//
// Each icon is a path plus the viewBox edge it was authored against.
// IconButton scales from that box, so paths of different sizes mix freely.
pragma Singleton
import QtQuick

QtObject {
    // Source: icons8 "refresh", 30x30 viewBox.
    readonly property int refreshViewBox: 30
    readonly property string refresh:
        "M 15 3 C 12.031398 3 9.3028202 4.0834384 7.2070312 5.875 " +
        "A 1.0001 1.0001 0 1 0 8.5058594 7.3945312 " +
        "C 10.25407 5.9000929 12.516602 5 15 5 " +
        "C 20.19656 5 24.450989 8.9379267 24.951172 14 " +
        "L 22 14 L 26 20 L 30 14 L 26.949219 14 " +
        "C 26.437925 7.8516588 21.277839 3 15 3 z " +
        "M 4 10 L 0 16 L 3.0507812 16 " +
        "C 3.562075 22.148341 8.7221607 27 15 27 " +
        "C 17.968602 27 20.69718 25.916562 22.792969 24.125 " +
        "A 1.0001 1.0001 0 1 0 21.494141 22.605469 " +
        "C 19.74593 24.099907 17.483398 25 15 25 " +
        "C 9.80344 25 5.5490109 21.062074 5.0488281 16 " +
        "L 8 16 L 4 10 z"
}
