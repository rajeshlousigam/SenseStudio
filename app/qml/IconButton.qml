import QtQuick
import QtQuick.Controls.Basic

// 32px toolbar button; `active` gives the cyan TMS toolbar highlight.
AbstractButton {
    id: control
    property url iconSource
    property int iconSize: 18
    property bool active: false
    property string tip: ""

    focusPolicy: Qt.NoFocus
    implicitWidth: 32
    implicitHeight: 32
    hoverEnabled: true

    ToolTip.visible: tip !== "" && hovered
    ToolTip.delay: 600
    ToolTip.text: tip

    contentItem: Item {
        Image {
            anchors.centerIn: parent
            source: control.iconSource
            sourceSize: Qt.size(control.iconSize, control.iconSize)
            opacity: control.enabled ? 1 : 0.35
        }
    }

    background: Rectangle {
        radius: Theme.radiusMd
        color: control.active ? Theme.withAlpha(Theme.accent, 0.15)
             : control.down ? Theme.pressed
             : control.hovered ? Theme.hover : "transparent"
        border.width: control.active ? 1 : 0
        border.color: Theme.withAlpha(Theme.accent, 0.4)
        Behavior on color { ColorAnimation { duration: Theme.durationFast } }
    }
}
