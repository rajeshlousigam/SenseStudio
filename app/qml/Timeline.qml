pragma ComponentBehavior: Bound

import QtQuick
import SenseStudio

// Whole-clip timeline: keyframe filmstrip, draggable in/out range, playhead.
Rectangle {
    id: tl
    property var clipData: null
    property real position: 0          // seconds
    readonly property real duration: clipData ? clipData.duration : 0
    readonly property real inPoint: clipData ? clipData.inPoint : 0
    readonly property real outPoint: clipData ? clipData.outPoint : 0

    signal seek(real time)
    signal setIn(real time)
    signal setOut(real time)

    implicitHeight: 104
    color: Theme.sidebar

    function xFor(t) { return duration > 0 ? strip.x + (t / duration) * strip.width : strip.x }
    function timeFor(x) { return duration > 0 ? Math.max(0, Math.min(duration, (x - strip.x) / strip.width * duration)) : 0 }

    // Re-request the strip only after resizing settles; each size is one FFmpeg pass, then cached.
    property int stripCount: 16
    onWidthChanged: stripTimer.restart()
    Timer {
        id: stripTimer
        interval: 400
        onTriggered: tl.stripCount = Math.max(4, Math.min(60, Math.round(strip.width / 128)))
    }

    // Time ruler.
    Repeater {
        model: tl.duration > 0 ? 9 : 0
        delegate: Text {
            required property int index
            x: tl.xFor(tl.duration * index / 8) - (index === 8 ? width : index === 0 ? 0 : width / 2)
            y: 6
            text: Theme.formatDuration(tl.duration * index / 8)
            color: Theme.textMuted
            font.pixelSize: 10
            font.family: Theme.monoFamily
        }
    }

    Rectangle {
        id: strip
        x: Theme.s4
        y: 24
        width: parent.width - Theme.s4 * 2
        height: 52
        radius: Theme.radiusMd
        color: Theme.inset
        clip: true

        Image {
            id: filmstrip
            anchors.fill: parent
            asynchronous: true
            fillMode: Image.Stretch
            source: tl.clipData ? tl.clipData.thumb.replace("image://thumbs/", "image://thumbs/strip/" + tl.stripCount + "/") : ""
            opacity: filmstrip.status === Image.Ready ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.durationFast } }
        }

        // Dim what is outside the in/out range.
        Rectangle { x: 0; width: tl.xFor(tl.inPoint) - strip.x; height: parent.height; color: Qt.rgba(0, 0, 0, 0.62) }
        Rectangle {
            x: tl.xFor(tl.outPoint) - strip.x
            width: parent.width - x
            height: parent.height
            color: Qt.rgba(0, 0, 0, 0.62)
        }
        Rectangle {
            x: tl.xFor(tl.inPoint) - strip.x
            width: tl.xFor(tl.outPoint) - tl.xFor(tl.inPoint)
            height: parent.height
            color: "transparent"
            border.width: 2
            border.color: Theme.accentLight
            radius: Theme.radiusMd
        }
    }

    MouseArea {
        anchors.fill: parent
        onPressed: mouse => tl.seek(tl.timeFor(mouse.x))
        onPositionChanged: mouse => { if (pressed) tl.seek(tl.timeFor(mouse.x)) }
    }

    // In/out handles sit above the scrub area so they win the drag.
    Repeater {
        model: [{ edge: "in" }, { edge: "out" }]
        delegate: Rectangle {
            id: handle
            required property var modelData
            readonly property bool isIn: modelData.edge === "in"
            x: tl.xFor(isIn ? tl.inPoint : tl.outPoint) - width / 2
            y: strip.y - 4
            width: 10
            height: strip.height + 8
            radius: 3
            color: dragArea.pressed || dragArea.containsMouse ? "white" : Theme.accentLight
            visible: tl.duration > 0
            MouseArea {
                id: dragArea
                anchors.fill: parent
                anchors.margins: -4
                hoverEnabled: true
                cursorShape: Qt.SizeHorCursor
                onPositionChanged: mouse => {
                    if (!pressed) return
                    const t = tl.timeFor(mapToItem(tl, mouse.x, 0).x)
                    handle.isIn ? tl.setIn(t) : tl.setOut(t)
                }
            }
        }
    }

    // Playhead.
    Rectangle {
        x: tl.xFor(tl.position) - 1
        y: strip.y - 6
        width: 2
        height: strip.height + 12
        color: "white"
        visible: tl.duration > 0
        Rectangle { anchors.horizontalCenter: parent.horizontalCenter; y: -4; width: 10; height: 6; radius: 2; color: "white" }
    }

    Text {
        anchors.centerIn: strip
        visible: !tl.clipData
        text: qsTr("Add clips from the Library to start editing")
        color: Theme.textMuted
        font.pixelSize: Theme.fontBase
    }
}
