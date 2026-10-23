pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import SenseStudio

Rectangle {
    id: panel
    property var item: null
    required property JobQueue queue
    signal addToEdit(var item)

    color: Theme.canvas

    Rectangle { width: parent.width; height: 1; color: Theme.borderSubtle }

    Text {
        anchors.centerIn: parent
        visible: !panel.item
        text: qsTr("No file selected")
        color: Theme.textMuted
        font.pixelSize: Theme.fontBase
    }

    RowLayout {
        anchors.fill: parent
        anchors.margins: Theme.s5
        spacing: Theme.s6
        visible: !!panel.item

        ColumnLayout {
            Layout.preferredWidth: 360
            Layout.fillHeight: true
            spacing: Theme.s2

            Text {
                Layout.fillWidth: true
                text: panel.item ? panel.item.name : ""
                elide: Text.ElideMiddle
                color: Theme.textPrimary
                font.pixelSize: Theme.fontLg
                font.weight: Font.Bold
            }
            Text {
                Layout.fillWidth: true
                text: panel.item ? panel.item.folder : ""
                elide: Text.ElideMiddle
                color: Theme.textMuted
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSm
            }
            StatusBadge {
                Layout.topMargin: Theme.s1
                status: panel.item ? panel.item.status : 0
                text: panel.item ? panel.item.statusText : ""
            }
            Text {
                Layout.fillWidth: true
                Layout.fillHeight: true
                text: panel.item ? panel.item.details : ""
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                color: panel.item && panel.item.status === MediaLibrary.Failed ? Theme.danger : Theme.textSecondary
                font.pixelSize: Theme.fontBase
                verticalAlignment: Text.AlignTop
            }
            RowLayout {
                spacing: Theme.s2
                SButton {
                    text: qsTr("Add to Edit")
                    iconSource: "icons/scissors.svg"
                    // Links must be made StepOut Ready (downloaded) before they can be trimmed.
                    enabled: !!panel.item && panel.item.duration > 0 && (!panel.item.remote || panel.item.output !== "")
                    // Edit the converted file when there is one, so exports start from a clean source.
                    onClicked: {
                        const it = panel.item
                        panel.addToEdit(it.output !== "" ? Object.assign({}, it, { path: it.output, name: it.output.split(/[\\/]/).pop() }) : it)
                    }
                }
                SButton {
                    visible: !!panel.item && panel.item.output !== ""
                    text: qsTr("Show converted file")
                    iconSource: "icons/folder.svg"
                    onClicked: panel.queue.reveal(panel.item.output)
                }
            }
        }

        Rectangle { Layout.fillHeight: true; implicitWidth: 1; color: Theme.borderSubtle }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: Theme.s2

            Text {
                text: qsTr("STEPOUT READY CHECK")
                color: Theme.textSecondary
                font.pixelSize: Theme.fontXs
                font.weight: Font.Bold
                font.letterSpacing: 1.2
            }

            Row {
                visible: !!panel.item && panel.item.status !== MediaLibrary.Probing
                         && panel.item.status !== MediaLibrary.Failed
                         && !panel.item.violations.some(v => v.blocking)
                spacing: Theme.s2
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: Theme.success }
                Text {
                    text: qsTr("Progressive MP4 · H.264 8-bit SDR · constant frame rate · AAC: meets the spec")
                    color: Theme.success
                    font.pixelSize: Theme.fontBase
                }
            }

            ListView {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                spacing: 6
                boundsBehavior: Flickable.StopAtBounds
                model: panel.item ? panel.item.violations : []

                delegate: Row {
                    id: rule
                    required property var modelData
                    width: ListView.view.width
                    spacing: Theme.s2
                    Rectangle {
                        anchors.verticalCenter: parent.verticalCenter
                        width: 8
                        height: 8
                        radius: 4
                        color: !rule.modelData.blocking ? Theme.textMuted
                             : rule.modelData.fix === "remux" ? Theme.info
                             : rule.modelData.fix === "audio_encode" ? Theme.warning : Theme.orange
                    }
                    Text {
                        width: Math.min(implicitWidth, rule.width - 8 - Theme.s2 - (rule.modelData.blocking ? 0 : 70))
                        text: rule.modelData.message
                        elide: Text.ElideRight
                        color: rule.modelData.blocking ? Theme.textPrimary : Theme.textSecondary
                        font.pixelSize: Theme.fontBase
                    }
                    Text {
                        visible: !rule.modelData.blocking
                        text: qsTr("advisory")
                        color: Theme.textMuted
                        font.pixelSize: Theme.fontXs
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }
            }
        }
    }
}
