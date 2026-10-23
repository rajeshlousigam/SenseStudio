pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtMultimedia
import SenseStudio

// Trim + merge: order the clips, set in/out on each, export one StepOut Ready file.
Rectangle {
    id: edit
    required property SequenceModel sequence
    required property MediaLibrary library
    required property JobQueue queue

    color: Theme.canvas

    property int currentRow: -1
    readonly property var current: sequence.revision >= 0 && currentRow >= 0 && currentRow < sequence.count
        ? sequence.get(currentRow) : null
    readonly property real now: playerPanel.player.position / 1000
    readonly property bool active: visible && !!current
    property string message: ""

    Connections {
        target: edit.sequence
        function onCountChanged() {
            if (edit.currentRow >= edit.sequence.count) edit.currentRow = edit.sequence.count - 1
            else if (edit.currentRow < 0 && edit.sequence.count > 0) edit.currentRow = 0
        }
    }

    Connections {
        target: playerPanel.player
        // A paused seek makes the backend present the real frame at the in point.
        function onMediaStatusChanged() {
            if (playerPanel.player.mediaStatus === MediaPlayer.LoadedMedia && edit.current)
                playerPanel.player.setPosition(Math.max(edit.current.inPoint * 1000, 1))
        }
        // Preview stops at the out point.
        function onPositionChanged() {
            if (edit.current && playerPanel.playing && edit.now >= edit.current.outPoint)
                playerPanel.player.pause()
        }
    }

    Shortcut { sequence: "I"; enabled: edit.active; onActivated: edit.sequence.setInPoint(edit.currentRow, edit.now) }
    Shortcut { sequence: "O"; enabled: edit.active; onActivated: edit.sequence.setOutPoint(edit.currentRow, edit.now) }

    EngineTask {
        id: renderTask
        onFinished: (ok, result) => {
            if (ok && result.output_file) {
                edit.library.addUrls([Util.fileUrl(result.output_file)])
                edit.message = qsTr("Exported %1 (added to Library)").arg(Util.fileName(result.output_file))
            } else {
                edit.message = renderTask.message || qsTr("Export failed")
            }
        }
    }

    function exportClip() {
        if (sequence.count === 0 || renderTask.running) return
        const specPath = Util.joinPath(Util.makeTempSubdir("render"), "spec.json")
        if (!Util.writeJson(specPath, { segments: sequence.segments(), preset: library.preset })) {
            message = qsTr("Could not write the export spec")
            return
        }
        const base = (sequence.count === 1 ? Util.baseName(sequence.get(0).path) + " trimmed" : "Merged") + " " + Util.timestamp()
        const out = Util.uniquePath(queue.outputDir, base, "mp4")
        message = ""
        renderTask.start(["render", specPath, "--output", out, "--events", "--encoder", queue.encoder])
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // Clip order + export
        Rectangle {
            Layout.preferredWidth: 320
            Layout.fillHeight: true
            color: Theme.sidebar

            ColumnLayout {
                anchors.fill: parent
                spacing: 0

                RowLayout {
                    Layout.fillWidth: true
                    Layout.margins: Theme.s4
                    Layout.bottomMargin: Theme.s2
                    Text { text: qsTr("CLIPS"); color: Theme.textSecondary; font.pixelSize: Theme.fontXs; font.weight: Font.Bold; font.letterSpacing: 1.2 }
                    Text { text: edit.sequence.count + "  ·  " + Theme.formatDuration(edit.sequence.totalDuration); color: Theme.textMuted; font.pixelSize: Theme.fontXs }
                    Item { Layout.fillWidth: true }
                    IconButton {
                        visible: edit.sequence.count > 0
                        enabled: !renderTask.running
                        iconSource: "icons/trash.svg"
                        iconSize: 16
                        tip: qsTr("Remove all clips")
                        onClicked: edit.sequence.clear()
                    }
                }

                ListView {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    clip: true
                    spacing: 2
                    model: edit.sequence
                    currentIndex: edit.currentRow
                    boundsBehavior: Flickable.StopAtBounds
                    delegate: ItemDelegate {
                        id: clipCard
                        required property int index
                        required property string name
                        required property string thumb
                        required property real inPoint
                        required property real outPoint
                        required property real clipDuration
                        width: ListView.view.width - Theme.s2 * 2
                        x: Theme.s2
                        height: 64
                        hoverEnabled: true
                        focusPolicy: Qt.NoFocus
                        onClicked: edit.currentRow = index
                        background: Rectangle {
                            radius: Theme.radiusLg
                            color: clipCard.ListView.isCurrentItem ? Theme.brandSubtle : clipCard.hovered ? Theme.hover : "transparent"
                            border.width: clipCard.ListView.isCurrentItem ? 1 : 0
                            border.color: Theme.withAlpha(Theme.brand, 0.45)
                        }
                        contentItem: Item {
                            Text {
                                id: orderLabel
                                anchors.left: parent.left
                                anchors.leftMargin: Theme.s2
                                anchors.verticalCenter: parent.verticalCenter
                                width: 14
                                text: clipCard.index + 1
                                color: Theme.textMuted
                                font.pixelSize: Theme.fontSm
                                font.weight: Font.Bold
                            }
                            Rectangle {
                                id: clipThumb
                                anchors.left: orderLabel.right
                                anchors.leftMargin: Theme.s2
                                anchors.verticalCenter: parent.verticalCenter
                                width: 80; height: 45; radius: 6; color: Theme.inset; clip: true
                                Image { anchors.fill: parent; source: clipCard.thumb; asynchronous: true; fillMode: Image.PreserveAspectCrop; sourceSize.width: 160 }
                            }
                            Column {
                                anchors.left: clipThumb.right
                                anchors.leftMargin: Theme.s3
                                anchors.right: parent.right
                                anchors.rightMargin: Theme.s2
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 3
                                Text { width: parent.width; text: clipCard.name; elide: Text.ElideMiddle; color: Theme.textPrimary; font.pixelSize: Theme.fontBase; font.weight: Font.Medium }
                                Text {
                                    text: Theme.formatDuration(clipCard.inPoint) + " – " + Theme.formatDuration(clipCard.outPoint)
                                          + "  ·  " + Theme.formatDuration(clipCard.clipDuration)
                                    color: Theme.textMuted
                                    font.pixelSize: Theme.fontSm
                                }
                            }
                            Row {
                                anchors.right: parent.right
                                anchors.rightMargin: Theme.s1
                                anchors.verticalCenter: parent.verticalCenter
                                visible: clipCard.hovered && !renderTask.running
                                IconButton { iconSource: "icons/up.svg"; iconSize: 14; implicitWidth: 24; implicitHeight: 24; enabled: clipCard.index > 0; tip: qsTr("Move up")
                                    onClicked: { edit.sequence.move(clipCard.index, clipCard.index - 1); edit.currentRow = clipCard.index - 1 } }
                                IconButton { iconSource: "icons/down.svg"; iconSize: 14; implicitWidth: 24; implicitHeight: 24; enabled: clipCard.index < edit.sequence.count - 1; tip: qsTr("Move down")
                                    onClicked: { edit.sequence.move(clipCard.index, clipCard.index + 1); edit.currentRow = clipCard.index + 1 } }
                                IconButton { iconSource: "icons/close.svg"; iconSize: 14; implicitWidth: 24; implicitHeight: 24; tip: qsTr("Remove")
                                    onClicked: edit.sequence.removeAt(clipCard.index) }
                            }
                        }
                    }
                    Text {
                        anchors.centerIn: parent
                        width: parent.width - 48
                        visible: edit.sequence.count === 0
                        horizontalAlignment: Text.AlignHCenter
                        wrapMode: Text.WordWrap
                        text: qsTr("Pick a video in the Library and press “Add to Edit”. Clips are merged top to bottom.")
                        color: Theme.textMuted
                        font.pixelSize: Theme.fontBase
                    }
                }

                Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.borderSubtle }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.margins: Theme.s4
                    spacing: Theme.s2

                    SButton {
                        Layout.fillWidth: true
                        variant: "primary"
                        iconSource: "icons/export.svg"
                        text: edit.sequence.count > 1 ? qsTr("Merge & export StepOut Ready clip") : qsTr("Export StepOut Ready clip")
                        enabled: edit.sequence.count > 0 && !renderTask.running
                        onClicked: edit.exportClip()
                    }
                    ColumnLayout {
                        Layout.fillWidth: true
                        visible: renderTask.running
                        spacing: 4
                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: 4
                            radius: 2
                            color: Theme.elevated
                            Rectangle { width: parent.width * renderTask.progress; height: parent.height; radius: 2; color: Theme.accent }
                        }
                        RowLayout {
                            Text { text: Math.round(renderTask.progress * 100) + "%  " + renderTask.speedText; color: Theme.textSecondary; font.pixelSize: Theme.fontSm }
                            Item { Layout.fillWidth: true }
                            SButton { text: qsTr("Cancel"); variant: "danger"; implicitHeight: 26; onClicked: renderTask.cancel() }
                        }
                    }
                    Text {
                        Layout.fillWidth: true
                        visible: edit.message !== "" && !renderTask.running
                        text: edit.message
                        wrapMode: Text.WordWrap
                        color: Theme.textSecondary
                        font.pixelSize: Theme.fontSm
                    }
                }
            }
        }

        Rectangle { Layout.fillHeight: true; implicitWidth: 1; color: Theme.borderSubtle }

        // Player + trim
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            PlayerPanel {
                id: playerPanel
                Layout.fillWidth: true
                Layout.fillHeight: true
                item: edit.current
                showPoster: false
            }

            Timeline {
                Layout.fillWidth: true
                clipData: edit.current
                position: edit.now
                onSeek: t => playerPanel.player.setPosition(t * 1000)
                onSetIn: t => edit.sequence.setInPoint(edit.currentRow, t)
                onSetOut: t => edit.sequence.setOutPoint(edit.currentRow, t)
            }

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: 52
                color: Theme.sidebar
                visible: !!edit.current

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.s4
                    anchors.rightMargin: Theme.s4
                    spacing: Theme.s3

                    Text { text: qsTr("In"); color: Theme.textSecondary; font.pixelSize: Theme.fontBase }
                    Text {
                        text: edit.current ? Theme.formatTimecode(edit.current.inPoint * 1000, edit.current.fps) : ""
                        color: Theme.textPrimary; font.family: Theme.monoFamily; font.pixelSize: Theme.fontBase
                    }
                    SButton { text: qsTr("Set in (I)"); implicitHeight: 30; onClicked: edit.sequence.setInPoint(edit.currentRow, edit.now) }

                    Item { implicitWidth: Theme.s4 }

                    Text { text: qsTr("Out"); color: Theme.textSecondary; font.pixelSize: Theme.fontBase }
                    Text {
                        text: edit.current ? Theme.formatTimecode(edit.current.outPoint * 1000, edit.current.fps) : ""
                        color: Theme.textPrimary; font.family: Theme.monoFamily; font.pixelSize: Theme.fontBase
                    }
                    SButton { text: qsTr("Set out (O)"); implicitHeight: 30; onClicked: edit.sequence.setOutPoint(edit.currentRow, edit.now) }

                    Item { Layout.fillWidth: true }
                    SButton {
                        text: qsTr("Reset")
                        implicitHeight: 30
                        onClicked: { edit.sequence.setInPoint(edit.currentRow, 0); edit.sequence.setOutPoint(edit.currentRow, edit.current.duration) }
                    }
                }
            }
        }
    }
}
