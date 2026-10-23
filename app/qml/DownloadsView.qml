pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import SenseStudio

// Download links (YouTube and most sites) with yt-dlp, tick the ones you want,
// order them, and merge them into one StepOut Ready clip. Only the merged clip
// goes to the Library.
Rectangle {
    id: view
    required property MediaLibrary library
    required property JobQueue queue

    color: Theme.canvas

    // url, status (Queued|Downloading|Done|Failed|Cancelled), progress, detail, file, selected
    ListModel { id: items }
    property string message: ""
    // Dev/test hook (--merge): merge everything downloaded once the queue drains.
    property bool autoMerge: false
    property var batch: []

    readonly property int selectedCount: {
        let n = 0
        for (let i = 0; i < items.count; ++i) if (items.get(i).selected && items.get(i).status === "Done") ++n
        return n + revision * 0
    }
    property int revision: 0 // bumped on edits so selectedCount re-evaluates

    function addLinks(text) {
        let added = 0
        for (const raw of text.split(/\s+/)) {
            const url = raw.trim()
            if (!/^https?:\/\/\S+$/i.test(url)) continue
            let dup = false
            for (let i = 0; i < items.count; ++i) if (items.get(i).url === url) dup = true
            if (dup) continue
            items.append({ url: url, status: "Queued", progress: 0, detail: "", file: "", selected: true })
            ++added
        }
        message = added === 0 ? qsTr("No new http(s) links found") : ""
        revision++
        startNext()
        return added
    }

    function startNext() {
        if (downloadTask.running || mergeTask.running) return
        const urls = []
        for (let i = 0; i < items.count; ++i) if (items.get(i).status === "Queued") urls.push(items.get(i).url)
        if (urls.length === 0) return
        batch = urls
        downloadTask.start(["download", "--events", "--output-dir", Util.joinPath(queue.outputDir, "Downloads"),
                            "--preset", library.preset, "--"].concat(urls))
    }

    function rowFor(url) {
        for (let i = 0; i < items.count; ++i) if (items.get(i).url === url) return i
        return -1
    }

    function mergeSelected() {
        const files = []
        for (let i = 0; i < items.count; ++i) {
            const it = items.get(i)
            if (it.selected && it.status === "Done" && it.file !== "") files.push(it.file)
        }
        if (files.length === 0 || mergeTask.running) return
        const specPath = Util.joinPath(Util.makeTempSubdir("merge"), "spec.json")
        if (!Util.writeJson(specPath, { segments: files.map(p => ({ path: p })), preset: library.preset })) {
            message = qsTr("Could not write the merge spec")
            return
        }
        const base = (files.length === 1 ? Util.baseName(files[0]) : "Merged " + files.length + " videos") + " " + Util.timestamp()
        message = ""
        mergeTask.start(["render", specPath, "--output", Util.uniquePath(queue.outputDir, base, "mp4"), "--events", "--encoder", queue.encoder])
    }

    EngineTask {
        id: downloadTask
        onEvent: ev => {
            const row = view.rowFor(view.batch[ev.index])
            if (row < 0) return
            if (ev.event === "start") {
                items.setProperty(row, "status", "Downloading")
            } else if (ev.event === "progress") {
                items.setProperty(row, "progress", ev.fraction || 0)
                items.setProperty(row, "detail", (ev.speed_text || "") + (ev.eta ? "  ·  ETA " + ev.eta : ""))
            } else if (ev.event === "result") {
                const ok = ev.status === "DOWNLOADED" && ev.files.length > 0
                items.setProperty(row, "status", ok ? "Done" : ev.status === "CANCELLED" ? "Cancelled" : "Failed")
                items.setProperty(row, "progress", ok ? 1 : 0)
                items.setProperty(row, "detail", ok ? "" : ev.details)
                if (ok) items.setProperty(row, "file", ev.files[0])
                view.revision++
            }
        }
        // Links added while this batch ran are picked up next.
        onFinished: ok => {
            if (!ok && downloadTask.message !== "") view.message = downloadTask.message
            for (let i = 0; i < items.count; ++i)
                if (items.get(i).status === "Downloading") items.setProperty(i, "status", "Cancelled")
            view.startNext()
            if (view.autoMerge && !downloadTask.running) { view.autoMerge = false; view.mergeSelected() }
        }
    }
    EngineTask {
        id: mergeTask
        onFinished: (ok, result) => {
            if (ok && result.output_file) {
                view.library.addUrls([Util.fileUrl(result.output_file)])
                view.message = qsTr("Created %1 (added to Library)").arg(Util.fileName(result.output_file))
            } else {
                view.message = mergeTask.message || qsTr("Merge failed")
            }
            view.startNext()
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: Theme.s6
        spacing: Theme.s4

        Text { text: qsTr("Download & merge"); color: Theme.textPrimary; font.pixelSize: Theme.fontXl + 4; font.weight: Font.Bold }
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.textSecondary
            font.pixelSize: Theme.fontMd
            text: qsTr("Paste YouTube (or other) links and download. Tick the videos you want, put them in order, and create one StepOut Ready clip. Only download videos you own or have permission to use.")
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s3

            Rectangle {
                Layout.fillWidth: true
                implicitHeight: 84
                radius: Theme.radiusControl
                color: Theme.inset
                border.color: urlInput.activeFocus ? Theme.withAlpha(Theme.accent, 0.6) : Theme.border
                ScrollView {
                    anchors.fill: parent
                    anchors.margins: 2
                    TextArea {
                        id: urlInput
                        wrapMode: TextEdit.WrapAnywhere
                        color: Theme.textPrimary
                        font.family: Theme.monoFamily
                        font.pixelSize: Theme.fontBase
                        placeholderText: qsTr("https://www.youtube.com/watch?v=…   (one or more links)")
                        placeholderTextColor: Theme.textMuted
                        background: null
                    }
                }
            }
            SButton {
                Layout.preferredWidth: 160
                variant: "primary"
                iconSource: "icons/download.svg"
                text: qsTr("Download")
                onClicked: if (view.addLinks(urlInput.text) > 0) urlInput.text = ""
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: Theme.s2
            Text { text: qsTr("VIDEOS (%1)").arg(items.count); color: Theme.textSecondary; font.pixelSize: Theme.fontXs; font.weight: Font.Bold; font.letterSpacing: 1.2 }
            Item { Layout.fillWidth: true }
            Text { text: view.message; color: Theme.textSecondary; font.pixelSize: Theme.fontSm; elide: Text.ElideRight; Layout.maximumWidth: 520 }
            SButton {
                visible: downloadTask.running || mergeTask.running
                variant: "danger"
                text: qsTr("Cancel")
                onClicked: downloadTask.running ? downloadTask.cancel() : mergeTask.cancel()
            }
            SButton {
                variant: "primary"
                iconSource: "icons/scissors.svg"
                text: mergeTask.running ? qsTr("Creating clip %1%").arg(Math.round(mergeTask.progress * 100))
                    : view.selectedCount > 1 ? qsTr("Merge %1 selected into one clip").arg(view.selectedCount)
                    : qsTr("Make selected StepOut Ready")
                enabled: view.selectedCount > 0 && !mergeTask.running
                onClicked: view.mergeSelected()
            }
        }

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 4
            model: items
            boundsBehavior: Flickable.StopAtBounds
            delegate: Rectangle {
                id: row
                required property int index
                required property string url
                required property string status
                required property real progress
                required property string detail
                required property string file
                required property bool selected
                width: ListView.view.width
                height: 56
                radius: Theme.radiusLg
                color: Theme.surface
                border.color: Theme.borderSubtle

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: Theme.s3
                    anchors.rightMargin: Theme.s2
                    spacing: Theme.s3
                    SCheckBox {
                        enabled: row.status === "Done"
                        checked: row.selected && row.status === "Done"
                        onToggled: { items.setProperty(row.index, "selected", checked); view.revision++ }
                    }
                    Text { text: row.index + 1; color: Theme.textMuted; font.pixelSize: Theme.fontBase; font.weight: Font.Bold }
                    ColumnLayout {
                        Layout.fillWidth: true
                        spacing: 4
                        Text {
                            Layout.fillWidth: true
                            text: row.file !== "" ? Util.fileName(row.file) : row.url
                            elide: Text.ElideMiddle
                            color: Theme.textPrimary
                            font.pixelSize: Theme.fontBase
                        }
                        Rectangle {
                            Layout.fillWidth: true
                            implicitHeight: 3
                            radius: 1.5
                            color: Theme.inset
                            visible: row.status === "Downloading"
                            Rectangle { width: parent.width * row.progress; height: parent.height; radius: 1.5; color: Theme.accent }
                        }
                        Text {
                            Layout.fillWidth: true
                            visible: row.detail !== ""
                            text: row.detail
                            elide: Text.ElideRight
                            color: row.status === "Failed" ? Theme.danger : Theme.textMuted
                            font.pixelSize: Theme.fontSm
                        }
                    }
                    StatusBadge {
                        status: row.status === "Done" ? MediaLibrary.Converted : row.status === "Failed" ? MediaLibrary.Failed
                              : row.status === "Downloading" ? MediaLibrary.Converting : MediaLibrary.Queued
                        text: row.status === "Downloading" ? Math.round(row.progress * 100) + "%" : row.status
                    }
                    IconButton { iconSource: "icons/up.svg"; iconSize: 14; tip: qsTr("Move up"); enabled: row.index > 0
                        onClicked: { items.move(row.index, row.index - 1, 1); view.revision++ } }
                    IconButton { iconSource: "icons/down.svg"; iconSize: 14; tip: qsTr("Move down"); enabled: row.index < items.count - 1
                        onClicked: { items.move(row.index, row.index + 1, 1); view.revision++ } }
                    IconButton { iconSource: "icons/close.svg"; iconSize: 14; tip: qsTr("Remove from list"); enabled: row.status !== "Downloading"
                        onClicked: { items.remove(row.index); view.revision++ } }
                }
            }
            Text {
                anchors.centerIn: parent
                visible: items.count === 0
                text: qsTr("No videos yet")
                color: Theme.textMuted
                font.pixelSize: Theme.fontMd
            }
        }
    }
}
