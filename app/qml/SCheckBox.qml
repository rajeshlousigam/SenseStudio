import QtQuick
import QtQuick.Controls.Basic

CheckBox {
    id: control
    focusPolicy: Qt.NoFocus
    padding: 0
    spacing: Theme.s2
    font.pixelSize: Theme.fontBase

    indicator: Rectangle {
        x: control.leftPadding
        anchors.verticalCenter: parent.verticalCenter
        implicitWidth: 16
        implicitHeight: 16
        radius: 4
        color: control.checked ? Theme.brand : "transparent"
        border.color: control.checked ? Theme.brand : Theme.borderStrong
        Text { anchors.centerIn: parent; visible: control.checked; text: "✓"; color: "white"; font.pixelSize: 11; font.bold: true }
    }
    contentItem: Text {
        leftPadding: control.indicator.width + control.spacing
        text: control.text
        font: control.font
        color: control.enabled ? Theme.textPrimary : Theme.textMuted
        verticalAlignment: Text.AlignVCenter
    }
}
