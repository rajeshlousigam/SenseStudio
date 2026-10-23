import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import SenseStudio

ApplicationWindow {
    id: window
    width: 1440
    height: 900
    minimumWidth: 1100
    minimumHeight: 680
    visible: true
    title: "SenseStudio " + Qt.application.version
    color: Theme.canvas

    MediaLibrary { id: libraryModel }
    JobQueue { id: jobQueue; library: libraryModel }
    SequenceModel { id: sequenceModel }

    // 0 Library, 1 Edit, 2 Download
    property int view: 0

    property list<url> initialUrls: []
    property bool autoConvert: false
    property string outputOverride: ""
    property string initialLinks: ""
    property string initialDownloads: ""
    property bool autoMerge: false
    Component.onCompleted: {
        if (outputOverride !== "") jobQueue.overrideOutputDir(outputOverride)
        if (initialUrls.length > 0) libraryModel.addUrls(initialUrls)
        if (initialLinks !== "") libraryModel.addLinks(initialLinks)
        if (initialDownloads !== "") downloadsView.addLinks(initialDownloads)
    }

    Connections {
        target: libraryModel
        enabled: window.autoConvert
        function onProbingChanged() {
            if (!libraryModel.probing && !jobQueue.running) {
                window.autoConvert = false
                jobQueue.start()
            }
        }
    }

    property int currentRow: -1
    // Reading revision makes this re-evaluate whenever any item changes.
    readonly property var current: {
        return libraryModel.revision >= 0 && currentRow >= 0 && currentRow < libraryModel.count
            ? libraryModel.get(currentRow) : null
    }

    Connections {
        target: libraryModel
        function onCountChanged() {
            if (window.currentRow >= libraryModel.count)
                window.currentRow = libraryModel.count - 1
            else if (window.currentRow < 0 && libraryModel.count > 0)
                window.currentRow = 0
        }
    }

    header: Toolbar { library: libraryModel; queue: jobQueue }
    footer: StatusBar { library: libraryModel; queue: jobQueue }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        NavRail {
            Layout.fillHeight: true
            currentIndex: window.view
            onSelected: index => window.view = index
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: window.view

            RowLayout {
                spacing: 0

                LibraryPanel {
                    Layout.preferredWidth: 420
                    Layout.fillHeight: true
                    library: libraryModel
                    currentRow: window.currentRow
                    onActivated: row => window.currentRow = row
                }

                Rectangle { Layout.fillHeight: true; implicitWidth: 1; color: Theme.borderSubtle }

                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: 0

                    PlayerPanel {
                        Layout.fillWidth: true
                        Layout.fillHeight: true
                        item: window.current
                    }
                    DetailsPanel {
                        Layout.fillWidth: true
                        Layout.preferredHeight: 220
                        item: window.current
                        queue: jobQueue
                        onAddToEdit: item => {
                            const row = sequenceModel.addFromLibrary(item)
                            if (row >= 0) {
                                editView.currentRow = row
                                window.view = 1
                            }
                        }
                    }
                }
            }

            EditView {
                id: editView
                sequence: sequenceModel
                library: libraryModel
                queue: jobQueue
            }

            DownloadsView {
                id: downloadsView
                autoMerge: window.autoMerge
                library: libraryModel
                queue: jobQueue
            }
        }
    }

    DropArea {
        id: drop
        anchors.fill: parent
        onDropped: event => {
            if (event.hasUrls)
                libraryModel.addUrls(event.urls)
        }

        Rectangle {
            anchors.fill: parent
            anchors.margins: Theme.s3
            visible: drop.containsDrag
            radius: Theme.radiusXl
            color: Theme.withAlpha(Theme.brand, 0.10)
            border.width: 2
            border.color: Theme.withAlpha(Theme.accent, 0.6)
            Text {
                anchors.centerIn: parent
                text: qsTr("Drop videos or folders to import")
                color: Theme.textPrimary
                font.pixelSize: Theme.fontXl
                font.weight: Font.DemiBold
            }
        }
    }
}
