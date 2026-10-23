import QtQuick
import QtQuick.Layouts
import SenseStudio

Rectangle {
    id: bar
    required property MediaLibrary library
    required property JobQueue queue

    implicitHeight: 32
    color: Theme.sidebar

    Rectangle { width: parent.width; height: 1; color: Theme.borderSubtle }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: Theme.s4
        anchors.rightMargin: Theme.s4
        spacing: Theme.s4

        Text {
            color: Theme.textSecondary
            font.pixelSize: Theme.fontSm
            text: bar.library.probing ? qsTr("Analyzing files…")
                : bar.library.count === 0 ? qsTr("No files")
                : qsTr("%1 file(s) · %2 need fixing").arg(bar.library.count).arg(bar.library.fixableCount)
        }

        Item {
            Layout.fillWidth: true
            implicitHeight: parent.height
            visible: bar.queue.running

            Row {
                anchors.centerIn: parent
                spacing: Theme.s3
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: bar.queue.currentName
                    color: Theme.textPrimary
                    font.pixelSize: Theme.fontSm
                    elide: Text.ElideMiddle
                    width: Math.min(implicitWidth, 320)
                }
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 220
                    height: 4
                    radius: 2
                    color: Theme.elevated
                    Rectangle {
                        width: parent.width * bar.queue.currentProgress
                        height: parent.height
                        radius: 2
                        color: Theme.accent
                    }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontSm
                    text: Math.round(bar.queue.currentProgress * 100) + "%"
                          + (bar.queue.speed > 0 ? "  ·  " + bar.queue.speed.toFixed(1) + "× realtime" : "")
                          + "  ·  " + bar.queue.done + "/" + bar.queue.total
                }
            }
        }
        Item { Layout.fillWidth: true; visible: !bar.queue.running }

        Text {
            text: bar.queue.message
            color: Theme.textSecondary
            font.pixelSize: Theme.fontSm
            elide: Text.ElideLeft
            Layout.maximumWidth: 420
        }
        Text {
            visible: bar.queue.message.startsWith("Finished")
            text: qsTr("Open folder")
            color: Theme.accentLight
            font.pixelSize: Theme.fontSm
            font.underline: openArea.containsMouse
            MouseArea { id: openArea; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: bar.queue.revealOutput() }
        }
    }
}
