import QtQuick
import QtQuick.Controls.Basic

// Primary = StepOut purple→blue gradient; secondary = glass; danger = red outline.
Button {
    id: control
    property string variant: "secondary"
    property url iconSource: ""

    focusPolicy: Qt.NoFocus // keep Space/arrow shortcuts for the player
    implicitHeight: Theme.controlHeight
    leftPadding: Theme.s4
    rightPadding: Theme.s4
    font.pixelSize: Theme.fontBase
    font.weight: Font.DemiBold
    hoverEnabled: true

    contentItem: Row {
        spacing: Theme.s2
        Image {
            visible: control.iconSource.toString() !== ""
            source: control.iconSource
            sourceSize: Qt.size(16, 16)
            anchors.verticalCenter: parent.verticalCenter
            opacity: control.enabled ? 1 : 0.4
        }
        Text {
            text: control.text
            font: control.font
            color: !control.enabled ? Theme.textMuted
                 : control.variant === "danger" ? Theme.danger : Theme.textPrimary
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    background: Rectangle {
        radius: Theme.radiusControl
        opacity: control.enabled ? 1 : 0.5
        border.width: control.variant === "primary" ? 0 : 1
        border.color: control.variant === "danger" ? Theme.withAlpha(Theme.danger, 0.5)
                    : control.hovered ? Theme.borderStrong : Theme.border
        color: control.variant === "primary" ? "transparent"
             : control.down ? Theme.pressed
             : control.hovered ? Theme.hover : Qt.rgba(1, 1, 1, 0.03)
        gradient: control.variant === "primary" ? primaryGradient : null
        Behavior on color { ColorAnimation { duration: Theme.durationFast } }

        Gradient {
            id: primaryGradient
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: control.down ? "#6D28D9" : "#7C3AED" }
            GradientStop { position: 1; color: control.hovered ? Theme.brandHover : Theme.brand }
        }
    }
}
