pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic

Rectangle {
    id: rail
    property int currentIndex: 0
    signal selected(int index)

    implicitWidth: 64
    color: Theme.sidebar

    Rectangle { anchors.right: parent.right; width: 1; height: parent.height; color: Theme.borderSubtle }

    Column {
        anchors.horizontalCenter: parent.horizontalCenter
        y: Theme.s4
        spacing: Theme.s3
        Repeater {
            model: [
                { icon: "icons/library.svg", label: qsTr("Library"), key: "Ctrl+1" },
                { icon: "icons/scissors.svg", label: qsTr("Edit"), key: "Ctrl+2" },
                { icon: "icons/download.svg", label: qsTr("Download"), key: "Ctrl+3" },
            ]
            delegate: AbstractButton {
                id: navButton
                required property var modelData
                required property int index
                readonly property bool current: rail.currentIndex === index
                width: 52
                height: 54
                focusPolicy: Qt.NoFocus
                hoverEnabled: true
                onClicked: rail.selected(index)

                Shortcut { sequence: navButton.modelData.key; onActivated: rail.selected(navButton.index) }

                background: Rectangle {
                    radius: Theme.radiusLg
                    color: navButton.current ? "transparent" : navButton.hovered ? Theme.hover : "transparent"
                    gradient: navButton.current ? activeGradient : null
                    Gradient {
                        id: activeGradient
                        orientation: Gradient.Horizontal
                        GradientStop { position: 0; color: Theme.brand }
                        GradientStop { position: 1; color: Theme.accent }
                    }
                }
                contentItem: Column {
                    spacing: 3
                    topPadding: 6
                    Image { anchors.horizontalCenter: parent.horizontalCenter; source: navButton.modelData.icon; sourceSize: Qt.size(20, 20) }
                    Text {
                        anchors.horizontalCenter: parent.horizontalCenter
                        text: navButton.modelData.label
                        color: navButton.current ? "white" : Theme.textSecondary
                        font.pixelSize: 10
                        font.weight: Font.DemiBold
                    }
                }
            }
        }
    }
}
