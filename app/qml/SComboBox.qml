pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic

// Model entries are { text, value }; `currentValue` is bound by the caller.
ComboBox {
    id: control
    property string label: ""

    focusPolicy: Qt.NoFocus
    textRole: "text"
    valueRole: "value"
    implicitHeight: Theme.controlHeight
    implicitWidth: 150
    font.pixelSize: Theme.fontBase
    hoverEnabled: true

    contentItem: Text {
        leftPadding: Theme.s3
        rightPadding: control.indicator.width + Theme.s2
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
        font: control.font
        textFormat: Text.StyledText
        text: (control.label ? "<font color='" + Theme.textMuted + "'>" + control.label + "</font>  " : "") + control.displayText
        color: control.enabled ? Theme.textPrimary : Theme.textMuted
    }

    indicator: Text {
        x: control.width - width - Theme.s3
        anchors.verticalCenter: parent.verticalCenter
        text: "▾"
        color: Theme.textSecondary
        font.pixelSize: Theme.fontBase
    }

    background: Rectangle {
        radius: Theme.radiusControl
        color: control.hovered ? Theme.hover : Qt.rgba(1, 1, 1, 0.03)
        border.width: 1
        border.color: control.popup.visible ? Theme.withAlpha(Theme.accent, 0.5) : Theme.border
        opacity: control.enabled ? 1 : 0.5
    }

    delegate: ItemDelegate {
        id: entry
        required property var modelData
        required property int index
        width: control.width
        height: 32
        highlighted: control.highlightedIndex === index
        contentItem: Text {
            text: entry.modelData.text
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBase
            verticalAlignment: Text.AlignVCenter
        }
        background: Rectangle {
            radius: Theme.radiusMd - 2
            color: entry.highlighted ? Theme.brandSubtle : "transparent"
        }
    }

    popup: Popup {
        y: control.height + 4
        width: control.width
        padding: 4
        implicitHeight: contentItem.implicitHeight + 8
        contentItem: ListView {
            clip: true
            implicitHeight: contentHeight
            model: control.popup.visible ? control.delegateModel : null
            currentIndex: control.highlightedIndex
        }
        background: Rectangle {
            radius: Theme.radiusMd
            color: Theme.elevated
            border.color: Theme.border
        }
    }
}
