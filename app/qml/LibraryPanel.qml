pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import SenseStudio

Rectangle {
    id: panel
    required property MediaLibrary library
    property int currentRow: -1
    signal activated(int row)

    color: Theme.sidebar

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: Theme.s4
            Layout.bottomMargin: Theme.s2
            spacing: Theme.s2

            CheckBox {
                id: selectAll
                focusPolicy: Qt.NoFocus
                visible: panel.library.count > 0
                checked: panel.library.count > 0 && panel.library.checkedCount === panel.library.count
                onClicked: panel.library.setAllChecked(checked)
                padding: 0
                indicator: Rectangle {
                    implicitWidth: 16
                    implicitHeight: 16
                    radius: 4
                    color: selectAll.checked ? Theme.brand : "transparent"
                    border.color: selectAll.checked ? Theme.brand : Theme.borderStrong
                    Text { anchors.centerIn: parent; visible: selectAll.checked; text: "✓"; color: "white"; font.pixelSize: 11; font.bold: true }
                }
            }
            Text {
                text: qsTr("LIBRARY")
                color: Theme.textSecondary
                font.pixelSize: Theme.fontXs
                font.weight: Font.Bold
                font.letterSpacing: 1.2
            }
            Text {
                text: panel.library.count
                color: Theme.textMuted
                font.pixelSize: Theme.fontXs
            }
            Item { Layout.fillWidth: true }
            IconButton {
                visible: panel.library.count > 0
                enabled: !panel.library.locked
                iconSource: "icons/trash.svg"
                iconSize: 16
                tip: qsTr("Clear library")
                onClicked: panel.library.clear()
            }
        }

        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: panel.library
            spacing: 2
            reuseItems: true
            cacheBuffer: 600
            boundsBehavior: Flickable.StopAtBounds
            currentIndex: panel.currentRow
            ScrollBar.vertical: ScrollBar {
                width: 6
                contentItem: Rectangle { radius: 3; color: Qt.rgba(1, 1, 1, 0.12) }
            }

            delegate: MediaCard {
                width: ListView.view.width - Theme.s2 * 2
                x: Theme.s2
                selected: ListView.isCurrentItem
                library: panel.library
                onClicked: panel.activated(index)
            }

            Column {
                anchors.centerIn: parent
                visible: panel.library.count === 0
                spacing: Theme.s3
                width: parent.width - Theme.s6 * 2
                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: "icons/film.svg"
                    sourceSize: Qt.size(40, 40)
                    opacity: 0.5
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    text: qsTr("Drop match videos here")
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontLg
                    font.weight: Font.DemiBold
                }
                Text {
                    width: parent.width
                    horizontalAlignment: Text.AlignHCenter
                    wrapMode: Text.WordWrap
                    text: qsTr("Files and folders are checked against the StepOut Ready spec. Only what fails is converted, using the fastest safe fix.")
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontBase
                }
            }
        }
    }
}
