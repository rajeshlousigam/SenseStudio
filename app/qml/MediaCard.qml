import QtQuick
import QtQuick.Controls.Basic
import SenseStudio

// Fixed-height, layout-free delegate: anchors only, so recycling in the
// ListView stays cheap even with thousands of files.
ItemDelegate {
    id: card
    required property int index
    required property string name
    required property string thumb
    required property string resolution
    required property string codec
    required property real fps
    required property real duration
    required property real sizeBytes
    required property int status
    required property string statusText
    required property real progress
    required property bool marked
    property bool selected: false
    property MediaLibrary library

    height: 72
    hoverEnabled: true
    focusPolicy: Qt.NoFocus
    padding: 0

    background: Rectangle {
        radius: Theme.radiusLg
        color: card.selected ? Theme.brandSubtle : card.hovered ? Theme.hover : "transparent"
        border.width: card.selected ? 1 : 0
        border.color: Theme.withAlpha(Theme.brand, 0.45)
    }

    contentItem: Item {
        CheckBox {
            id: check
            anchors.left: parent.left
            anchors.leftMargin: Theme.s3
            anchors.verticalCenter: parent.verticalCenter
            focusPolicy: Qt.NoFocus
            padding: 0
            checked: card.marked
            onClicked: card.library.setChecked(card.index, checked)
            indicator: Rectangle {
                implicitWidth: 16
                implicitHeight: 16
                radius: 4
                color: check.checked ? Theme.brand : "transparent"
                border.color: check.checked ? Theme.brand : Theme.borderStrong
                Text { anchors.centerIn: parent; visible: check.checked; text: "✓"; color: "white"; font.pixelSize: 11; font.bold: true }
            }
        }

        Rectangle {
            id: thumbFrame
            anchors.left: check.right
            anchors.leftMargin: Theme.s3
            anchors.verticalCenter: parent.verticalCenter
            width: 96
            height: 54
            radius: Theme.radiusMd
            color: Theme.inset
            clip: true

            Image {
                id: poster
                anchors.fill: parent
                source: card.thumb
                asynchronous: true
                fillMode: Image.PreserveAspectCrop
                sourceSize.width: 192
                opacity: poster.status === Image.Ready ? 1 : 0
                Behavior on opacity { NumberAnimation { duration: Theme.durationFast } }
            }
            Text {
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: 3
                visible: card.duration > 0
                text: Theme.formatDuration(card.duration)
                color: "white"
                font.pixelSize: 10
                font.weight: Font.DemiBold
                style: Text.Outline
                styleColor: Qt.rgba(0, 0, 0, 0.6)
            }
        }

        Text {
            id: title
            anchors.left: thumbFrame.right
            anchors.leftMargin: Theme.s3
            anchors.right: badge.left
            anchors.rightMargin: Theme.s2
            anchors.top: thumbFrame.top
            anchors.topMargin: 4
            text: card.name
            elide: Text.ElideMiddle
            color: Theme.textPrimary
            font.pixelSize: Theme.fontBase
            font.weight: Font.Medium
        }

        Text {
            anchors.left: title.left
            anchors.right: parent.right
            anchors.rightMargin: Theme.s3
            anchors.top: title.bottom
            anchors.topMargin: 4
            elide: Text.ElideRight
            color: Theme.textMuted
            font.pixelSize: Theme.fontSm
            text: [card.resolution, card.codec.toUpperCase(), card.fps > 0 ? card.fps.toFixed(card.fps % 1 ? 2 : 0) + " fps" : "", Theme.formatSize(card.sizeBytes)]
                  .filter(s => s !== "").join("  ·  ")
        }

        StatusBadge {
            id: badge
            anchors.right: parent.right
            anchors.rightMargin: Theme.s3
            anchors.top: thumbFrame.top
            status: card.status
            text: card.status === MediaLibrary.Converting ? Math.round(card.progress * 100) + "%" : card.statusText
        }

        Rectangle {
            anchors.left: title.left
            anchors.right: parent.right
            anchors.rightMargin: Theme.s3
            anchors.bottom: thumbFrame.bottom
            height: 3
            radius: 1.5
            visible: card.status === MediaLibrary.Converting || card.status === MediaLibrary.Queued
            color: Theme.inset
            Rectangle {
                width: parent.width * Math.max(0, Math.min(1, card.progress))
                height: parent.height
                radius: parent.radius
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: Theme.brand }
                    GradientStop { position: 1; color: Theme.accent }
                }
            }
        }
    }
}
