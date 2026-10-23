import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts
import SenseStudio

Rectangle {
    id: bar
    required property MediaLibrary library
    required property JobQueue queue

    implicitHeight: 56
    color: Theme.sidebar

    // 1px StepOut panel accent along the bottom edge.
    Rectangle {
        anchors.bottom: parent.bottom
        width: parent.width
        height: 1
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Theme.withAlpha(Theme.accent, 0.5) }
            GradientStop { position: 0.5; color: Theme.withAlpha(Theme.brand, 0.3) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    FileDialog {
        id: fileDialog
        title: qsTr("Import videos")
        fileMode: FileDialog.OpenFiles
        nameFilters: [qsTr("Videos (*.mp4 *.m4v *.mov *.mkv *.webm *.avi *.mts *.m2ts *.ts *.mpg *.mpeg *.mxf *.wmv *.flv *.3gp)"), qsTr("All files (*)")]
        onAccepted: bar.library.addUrls(selectedFiles)
    }
    FolderDialog {
        id: folderDialog
        title: qsTr("Import a folder (searched recursively)")
        onAccepted: bar.library.addUrls([selectedFolder])
    }
    Popup {
        id: linkPopup
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: 90
        width: 640
        modal: true
        padding: Theme.s5
        onOpened: { linkInput.text = ""; linkInput.forceActiveFocus() }
        background: Rectangle { color: Theme.elevated; radius: Theme.radiusLg; border.color: Theme.border }
        contentItem: ColumnLayout {
            spacing: Theme.s3
            Text { text: qsTr("Check video links"); color: Theme.textPrimary; font.pixelSize: Theme.fontLg; font.weight: Font.Bold }
            Text {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                color: Theme.textSecondary
                font.pixelSize: Theme.fontBase
                text: qsTr("S3, CloudFront or any direct video link, one per line. Links are checked without downloading; only files that fail are downloaded and fixed when you press Make StepOut Ready.")
            }
            Rectangle {
                Layout.fillWidth: true
                implicitHeight: 140
                radius: Theme.radiusControl
                color: Theme.inset
                border.color: Theme.border
                ScrollView {
                    anchors.fill: parent
                    anchors.margins: 2
                    TextArea {
                        id: linkInput
                        wrapMode: TextEdit.WrapAnywhere
                        color: Theme.textPrimary
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontBase
                        placeholderText: "https://videos.stepoutplay.com/videos/match.mp4"
                        placeholderTextColor: Theme.textMuted
                        background: null
                    }
                }
            }
            RowLayout {
                Text { id: linkHint; color: Theme.warning; font.pixelSize: Theme.fontSm; text: "" }
                Item { Layout.fillWidth: true }
                SButton { text: qsTr("Cancel"); onClicked: linkPopup.close() }
                SButton {
                    variant: "primary"
                    text: qsTr("Check links")
                    onClicked: {
                        if (bar.library.addLinks(linkInput.text) > 0) { linkHint.text = ""; linkPopup.close() }
                        else linkHint.text = qsTr("No new http(s) links found")
                    }
                }
            }
        }
    }
    FolderDialog {
        id: outputDialog
        title: qsTr("Choose output folder")
        onAccepted: bar.queue.setOutputDirUrl(selectedFolder)
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.s5
        anchors.rightMargin: Theme.s4
        spacing: Theme.s2

        Row {
            Layout.rightMargin: Theme.s4
            spacing: 0
            Text { text: "Sense"; color: Theme.textPrimary; font.pixelSize: Theme.fontLg + 2; font.weight: Font.Bold }
            Text { text: "Studio"; color: Theme.accentLight; font.pixelSize: Theme.fontLg + 2; font.weight: Font.Bold }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                visible: Qt.application.version.indexOf("beta") >= 0
                width: betaLabel.implicitWidth + 10
                height: 18
                radius: 5
                color: Theme.withAlpha(Theme.warning, 0.15)
                border.color: Theme.withAlpha(Theme.warning, 0.4)
                Text { id: betaLabel; anchors.centerIn: parent; text: "BETA"; color: Theme.warning; font.pixelSize: 10; font.weight: Font.Bold; font.letterSpacing: 1 }
            }
        }

        SButton { text: qsTr("Add files"); iconSource: "icons/plus.svg"; onClicked: fileDialog.open() }
        SButton { text: qsTr("Add folder"); iconSource: "icons/folder.svg"; onClicked: folderDialog.open() }
        SButton { text: qsTr("Add link"); iconSource: "icons/download.svg"; onClicked: linkPopup.open() }

        Rectangle { implicitWidth: 1; implicitHeight: 24; color: Theme.border; Layout.leftMargin: Theme.s2; Layout.rightMargin: Theme.s2 }

        SComboBox {
            label: qsTr("Output")
            implicitWidth: 150
            enabled: !bar.queue.running
            model: [
                { text: "720p", value: "720" },
                { text: "1080p", value: "1080" },
                { text: "4K (2160p)", value: "2160" },
            ]
            // indexOfValue() is not reactive to the model, so resolve once it is populated.
            Component.onCompleted: currentIndex = indexOfValue(bar.library.preset)
            onActivated: bar.library.preset = currentValue
        }
        SComboBox {
            label: qsTr("Encoder")
            implicitWidth: 170
            enabled: !bar.queue.running
            model: [
                { text: qsTr("Auto (GPU)"), value: "auto" },
                { text: qsTr("Hardware"), value: "hardware" },
                { text: qsTr("Software x264"), value: "software" },
            ]
            Component.onCompleted: currentIndex = indexOfValue(bar.queue.encoder)
            onActivated: bar.queue.encoder = currentValue
        }
        SButton {
            Layout.maximumWidth: 260
            iconSource: "icons/folder.svg"
            text: bar.queue.outputDir.split(/[\\/]/).pop() || bar.queue.outputDir
            enabled: !bar.queue.running
            onClicked: outputDialog.open()
            ToolTip.visible: hovered
            ToolTip.delay: 500
            ToolTip.text: qsTr("Output folder: %1").arg(bar.queue.outputDir)
        }

        Item { Layout.fillWidth: true }

        SButton {
            visible: bar.queue.running
            variant: "danger"
            text: qsTr("Cancel")
            onClicked: bar.queue.cancel()
        }
        SButton {
            variant: "primary"
            enabled: !bar.queue.running && bar.library.fixableCount > 0 && !bar.library.probing
            text: {
                if (bar.queue.running) return qsTr("Converting %1/%2").arg(Math.min(bar.queue.done + 1, bar.queue.total)).arg(bar.queue.total)
                const n = bar.library.checkedCount > 0 ? qsTr("selected") : qsTr("all")
                return bar.library.fixableCount > 0 ? qsTr("Make StepOut Ready (%1)").arg(n) : qsTr("All files ready")
            }
            onClicked: bar.queue.start()
        }
    }
}
