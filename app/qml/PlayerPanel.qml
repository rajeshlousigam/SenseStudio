pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtMultimedia
import SenseStudio

// Hardware-decoded preview (AVFoundation/VideoToolbox on macOS, FFmpeg/D3D11 on
// Windows) rendered straight into the scene graph via VideoOutput.
Rectangle {
    id: root
    property var item: null
    property bool showOutput: false
    // The poster is taken 1s in, so the trim editor shows the real frame instead.
    property bool showPoster: true
    readonly property alias player: mediaPlayer

    readonly property bool hasOutput: !!item && item.output !== ""
    readonly property string itemPath: item ? item.path : ""
    readonly property bool remote: !!item && !!item.remote
    readonly property string proxyPath: itemPath !== "" && !remote ? Util.proxyPathFor(itemPath) : ""
    property bool proxyReady: false
    property bool useProxy: false
    readonly property url sourceUrl: !item ? ""
        : useProxy && proxyReady ? Util.fileUrl(proxyPath)
        : (showOutput && hasOutput ? item.outputUrl : item.fileUrl)
    readonly property real fps: item && item.fps > 0 ? item.fps : 30
    readonly property real frameMs: 1000 / fps
    readonly property bool playing: mediaPlayer.playbackState === MediaPlayer.PlayingState
    readonly property bool shortcutsActive: visible && !!item

    color: Theme.inset
    onSourceUrlChanged: mediaPlayer.stop()
    onHasOutputChanged: if (!hasOutput) showOutput = false
    onItemPathChanged: { useProxy = false; refreshProxy() }
    function refreshProxy() { proxyReady = Util.exists(proxyPath) }
    onVisibleChanged: if (!visible) mediaPlayer.pause()

    EngineTask {
        id: proxyTask
        onFinished: ok => {
            root.proxyReady = ok && Util.exists(root.proxyPath)
            root.useProxy = root.proxyReady
        }
    }

    function seekBy(ms) {
        mediaPlayer.setPosition(Math.max(0, Math.min(mediaPlayer.duration, mediaPlayer.position + ms)))
    }
    function stepFrame(direction) {
        mediaPlayer.pause()
        seekBy(direction * frameMs)
    }
    function toggle() {
        if (!item) return
        playing ? mediaPlayer.pause() : mediaPlayer.play()
    }

    MediaPlayer {
        id: mediaPlayer
        source: root.sourceUrl
        videoOutput: video
        audioOutput: AudioOutput { id: audio; volume: 0.8 }
        // Fall back to an existing proxy silently when the platform decoder refuses the original.
        onErrorOccurred: if (root.proxyReady && !root.useProxy) root.useProxy = true
    }

    Shortcut { sequences: ["Space", "K"]; enabled: root.shortcutsActive; onActivated: root.toggle() }
    Shortcut { sequence: "J"; enabled: root.shortcutsActive; onActivated: root.seekBy(-5000) }
    Shortcut { sequence: "L"; enabled: root.shortcutsActive; onActivated: root.seekBy(5000) }
    Shortcut { sequence: "Left"; enabled: root.shortcutsActive; onActivated: root.stepFrame(-1) }
    Shortcut { sequence: "Right"; enabled: root.shortcutsActive; onActivated: root.stepFrame(1) }
    Shortcut { sequence: "Shift+Left"; enabled: root.shortcutsActive; onActivated: root.seekBy(-1000) }
    Shortcut { sequence: "Shift+Right"; enabled: root.shortcutsActive; onActivated: root.seekBy(1000) }
    Shortcut { sequence: "Home"; enabled: root.shortcutsActive; onActivated: mediaPlayer.setPosition(0) }
    Shortcut { sequence: "End"; enabled: root.shortcutsActive; onActivated: mediaPlayer.setPosition(Math.max(0, mediaPlayer.duration - root.frameMs)) }
    Shortcut { sequence: "M"; enabled: root.shortcutsActive; onActivated: audio.muted = !audio.muted }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

            VideoOutput {
                id: video
                anchors.fill: parent
                anchors.margins: Theme.s4
                fillMode: VideoOutput.PreserveAspectFit
            }

            // The backend renders nothing until playback starts; show the cached poster meanwhile.
            Image {
                anchors.fill: video
                visible: root.showPoster && !!root.item && !root.playing && mediaPlayer.position === 0 && !root.showOutput
                source: root.item ? root.item.thumb : ""
                fillMode: Image.PreserveAspectFit
                asynchronous: true
            }

            MouseArea {
                anchors.fill: parent
                onClicked: root.toggle()
            }

            Column {
                anchors.centerIn: parent
                spacing: Theme.s2
                visible: !root.item || mediaPlayer.error !== MediaPlayer.NoError
                Image {
                    anchors.horizontalCenter: parent.horizontalCenter
                    source: "icons/film.svg"
                    sourceSize: Qt.size(44, 44)
                    opacity: 0.35
                }
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    horizontalAlignment: Text.AlignHCenter
                    width: Math.min(root.width - 80, 520)
                    wrapMode: Text.WordWrap
                    color: Theme.textSecondary
                    font.pixelSize: Theme.fontMd
                    text: !root.item ? qsTr("Select a video to preview")
                        : proxyTask.running ? qsTr("Creating preview copy… %1%").arg(Math.round(proxyTask.progress * 100))
                        : root.remote ? qsTr("This link can't be previewed here (%1). Checking and converting still work.").arg(mediaPlayer.errorString)
                        : qsTr("This platform's player can't open this file (%1). A 540p preview copy fixes that; converting works either way.").arg(mediaPlayer.errorString)
                }
                SButton {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: !!root.item && !root.remote && !proxyTask.running && !root.useProxy
                    variant: "primary"
                    text: qsTr("Create preview copy")
                    onClicked: proxyTask.start(["proxy", root.itemPath, "--output", root.proxyPath, "--events"])
                }
            }

            Row {
                anchors.top: parent.top
                anchors.right: parent.right
                anchors.margins: Theme.s5
                visible: root.hasOutput
                spacing: 2
                Repeater {
                    model: [{ text: qsTr("Source"), output: false }, { text: qsTr("Converted"), output: true }]
                    delegate: AbstractButton {
                        id: seg
                        required property var modelData
                        focusPolicy: Qt.NoFocus
                        implicitWidth: segLabel.implicitWidth + 20
                        implicitHeight: 26
                        onClicked: root.showOutput = seg.modelData.output
                        background: Rectangle {
                            radius: 6
                            color: root.showOutput === seg.modelData.output ? Theme.brand : Qt.rgba(0, 0, 0, 0.55)
                        }
                        contentItem: Text {
                            id: segLabel
                            text: seg.modelData.text
                            color: "white"
                            font.pixelSize: Theme.fontSm
                            font.weight: Font.DemiBold
                            horizontalAlignment: Text.AlignHCenter
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            implicitHeight: 92
            color: Theme.sidebar
            enabled: !!root.item

            Slider {
                id: scrub
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.leftMargin: Theme.s4
                anchors.rightMargin: Theme.s4
                anchors.topMargin: Theme.s2
                focusPolicy: Qt.NoFocus
                from: 0
                to: Math.max(1, mediaPlayer.duration)
                // Live scrubbing: seek while dragging, but don't fight the drag with position updates.
                onMoved: mediaPlayer.setPosition(value)
                Binding on value { when: !scrub.pressed; value: mediaPlayer.position }

                background: Rectangle {
                    x: scrub.leftPadding
                    y: scrub.topPadding + scrub.availableHeight / 2 - height / 2
                    width: scrub.availableWidth
                    height: scrub.hovered || scrub.pressed ? 6 : 4
                    radius: 3
                    color: Theme.elevated
                    Rectangle {
                        width: scrub.visualPosition * parent.width
                        height: parent.height
                        radius: 3
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: Theme.brand }
                            GradientStop { position: 1; color: Theme.accent }
                        }
                    }
                }
                handle: Rectangle {
                    x: scrub.leftPadding + scrub.visualPosition * (scrub.availableWidth - width)
                    y: scrub.topPadding + scrub.availableHeight / 2 - height / 2
                    width: 14
                    height: 14
                    radius: 7
                    color: "white"
                    visible: scrub.hovered || scrub.pressed
                }
            }

            RowLayout {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.bottom: parent.bottom
                anchors.margins: Theme.s4
                anchors.bottomMargin: Theme.s3
                spacing: Theme.s1

                Text {
                    Layout.preferredWidth: 220
                    font.family: Theme.monoFamily
                    font.pixelSize: Theme.fontBase
                    color: Theme.textPrimary
                    textFormat: Text.StyledText
                    text: Theme.formatTimecode(mediaPlayer.position, root.fps)
                          + "<font color='" + Theme.textMuted + "'>  /  " + Theme.formatTimecode(mediaPlayer.duration, root.fps) + "</font>"
                }

                Item { Layout.fillWidth: true }

                IconButton { iconSource: "icons/skip-back.svg"; tip: qsTr("Back 5s (J)"); onClicked: root.seekBy(-5000) }
                IconButton { iconSource: "icons/step-back.svg"; tip: qsTr("Previous frame (←)"); onClicked: root.stepFrame(-1) }
                IconButton {
                    implicitWidth: 44
                    implicitHeight: 44
                    iconSize: 22
                    active: true
                    iconSource: root.playing ? "icons/pause.svg" : "icons/play.svg"
                    tip: qsTr("Play / pause (Space, K)")
                    onClicked: root.toggle()
                }
                IconButton { iconSource: "icons/step-forward.svg"; tip: qsTr("Next frame (→)"); onClicked: root.stepFrame(1) }
                IconButton { iconSource: "icons/skip-forward.svg"; tip: qsTr("Forward 5s (L)"); onClicked: root.seekBy(5000) }

                Item { Layout.fillWidth: true }

                SComboBox {
                    implicitWidth: 92
                    model: [
                        { text: "0.25×", value: 0.25 }, { text: "0.5×", value: 0.5 }, { text: "1×", value: 1 },
                        { text: "1.5×", value: 1.5 }, { text: "2×", value: 2 },
                    ]
                    currentIndex: 2
                    onActivated: mediaPlayer.playbackRate = currentValue
                }
                IconButton {
                    iconSource: audio.muted ? "icons/mute.svg" : "icons/volume.svg"
                    tip: qsTr("Mute (M)")
                    onClicked: audio.muted = !audio.muted
                }
                Slider {
                    id: volume
                    Layout.preferredWidth: 90
                    focusPolicy: Qt.NoFocus
                    from: 0
                    to: 1
                    value: audio.volume
                    onMoved: audio.volume = value
                    background: Rectangle {
                        x: volume.leftPadding
                        y: volume.topPadding + volume.availableHeight / 2 - height / 2
                        width: volume.availableWidth
                        height: 4
                        radius: 2
                        color: Theme.elevated
                        Rectangle { width: volume.visualPosition * parent.width; height: parent.height; radius: 2; color: Theme.textSecondary }
                    }
                    handle: Rectangle {
                        x: volume.leftPadding + volume.visualPosition * (volume.availableWidth - width)
                        y: volume.topPadding + volume.availableHeight / 2 - height / 2
                        width: 12
                        height: 12
                        radius: 6
                        color: "white"
                    }
                }
            }
        }
    }
}
