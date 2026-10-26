pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Dialogs
import QtQuick.Layouts
import SenseStudio

// Guided YouTube sign-in. Google blocks sign-in inside embedded browsers, so the user
// signs in with their own browser and yt-dlp borrows that session (or a cookies.txt).
Popup {
    id: dialog
    property string browser: ""
    property string cookiesFile: ""
    // Set by the caller when a download was refused, to explain why the dialog appeared.
    property string reason: ""

    signal applied(string browser, string cookiesFile)
    signal cleared()

    readonly property var browsers: Util.installedBrowsers()
    readonly property string browserLabel: {
        for (const b of browsers) if (b.id === browser) return b.label
        return browser
    }
    readonly property url signInUrl: "https://accounts.google.com/ServiceLogin?service=youtube&continue=https%3A%2F%2Fwww.youtube.com%2F"
    property bool browserLaunched: false

    // Parented to the view it is declared in; modal still dims the whole window.
    anchors.centerIn: parent
    width: 600
    modal: true
    padding: Theme.s6
    onAboutToShow: browserLaunched = false
    background: Rectangle { color: Theme.elevated; radius: Theme.radiusXl; border.color: Theme.border }

    component StepLabel: Text {
        color: Theme.textSecondary
        font.pixelSize: Theme.fontXs
        font.weight: Font.Bold
        font.letterSpacing: 1.2
    }

    FileDialog {
        id: cookiesPicker
        title: qsTr("Choose a cookies.txt exported from a browser signed in to YouTube")
        nameFilters: [qsTr("Cookies file (*.txt)"), qsTr("All files (*)")]
        onAccepted: dialog.cookiesFile = Util.localPath(selectedFile)
    }

    contentItem: ColumnLayout {
        spacing: Theme.s3

        Text { text: qsTr("Sign in to YouTube"); color: Theme.textPrimary; font.pixelSize: Theme.fontXl; font.weight: Font.Bold }
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.textSecondary
            font.pixelSize: Theme.fontBase
            text: (dialog.reason !== "" ? dialog.reason + " " : "")
                  + qsTr("SenseStudio uses the YouTube sign-in from your own browser. It never sees your password; the browser's cookies are read only while downloading.")
        }

        StepLabel { text: qsTr("1. CHOOSE YOUR BROWSER"); Layout.topMargin: Theme.s2 }
        Flow {
            Layout.fillWidth: true
            spacing: Theme.s2
            Repeater {
                model: dialog.browsers
                delegate: AbstractButton {
                    id: choice
                    required property var modelData
                    readonly property bool selected: dialog.browser === modelData.id && dialog.cookiesFile === ""
                    focusPolicy: Qt.NoFocus
                    implicitWidth: choiceLabel.implicitWidth + 32
                    implicitHeight: 34
                    hoverEnabled: true
                    onClicked: { dialog.browser = choice.modelData.id; dialog.cookiesFile = ""; dialog.browserLaunched = false }
                    background: Rectangle {
                        radius: Theme.radiusControl
                        color: choice.selected ? Theme.brandSubtle : choice.hovered ? Theme.hover : "transparent"
                        border.width: 1
                        border.color: choice.selected ? Theme.withAlpha(Theme.brand, 0.7) : Theme.border
                    }
                    contentItem: Text {
                        id: choiceLabel
                        text: choice.modelData.label
                        color: Theme.textPrimary
                        font.pixelSize: Theme.fontBase
                        font.weight: Font.DemiBold
                        horizontalAlignment: Text.AlignHCenter
                        verticalAlignment: Text.AlignVCenter
                    }
                }
            }
            Text {
                visible: dialog.browsers.length === 0
                text: qsTr("No supported browser found. Use a cookies file below.")
                color: Theme.warning
                font.pixelSize: Theme.fontBase
            }
        }

        StepLabel { text: qsTr("2. SIGN IN THERE"); Layout.topMargin: Theme.s2 }
        RowLayout {
            spacing: Theme.s3
            SButton {
                enabled: dialog.browser !== "" && dialog.cookiesFile === ""
                text: dialog.browser !== "" ? qsTr("Open YouTube sign-in in %1").arg(dialog.browserLabel) : qsTr("Open YouTube sign-in")
                onClicked: dialog.browserLaunched = Util.openInBrowser(dialog.browser, dialog.signInUrl)
            }
            Text {
                Layout.fillWidth: true
                visible: dialog.browserLaunched
                wrapMode: Text.WordWrap
                text: qsTr("Sign in, wait until YouTube loads, then come back here.")
                color: Theme.success
                font.pixelSize: Theme.fontSm
            }
        }
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.textMuted
            font.pixelSize: Theme.fontSm
            text: Qt.platform.os === "osx"
                ? qsTr("On the first download macOS asks to let SenseStudio read the browser's cookies; choose Allow. Safari also needs Full Disk Access for SenseStudio (System Settings → Privacy & Security).")
                : qsTr("Chrome and Edge on Windows often block other apps from reading their sign-in. If that happens, use Firefox or a cookies file.")
        }

        StepLabel { text: qsTr("3. COME BACK AND RETRY"); Layout.topMargin: Theme.s2 }

        Rectangle { Layout.fillWidth: true; Layout.topMargin: Theme.s2; implicitHeight: 1; color: Theme.borderSubtle }

        Text { text: qsTr("Or use a cookies file"); color: Theme.textPrimary; font.pixelSize: Theme.fontMd; font.weight: Font.DemiBold }
        Text {
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            color: Theme.textMuted
            font.pixelSize: Theme.fontSm
            text: qsTr("Works with any browser: while signed in to YouTube, export cookies.txt with a browser extension (yt-dlp recommends “Get cookies.txt LOCALLY”) and pick the file. It overrides the browser choice.")
        }
        RowLayout {
            spacing: Theme.s3
            SButton { text: qsTr("Choose cookies.txt…"); iconSource: "icons/folder.svg"; onClicked: cookiesPicker.open() }
            Text {
                Layout.fillWidth: true
                visible: dialog.cookiesFile !== ""
                text: Util.fileName(dialog.cookiesFile)
                elide: Text.ElideMiddle
                color: Theme.textPrimary
                font.family: Theme.monoFamily
                font.pixelSize: Theme.fontSm
            }
            IconButton {
                visible: dialog.cookiesFile !== ""
                iconSource: "icons/close.svg"
                iconSize: 14
                tip: qsTr("Don't use the cookies file")
                onClicked: dialog.cookiesFile = ""
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.topMargin: Theme.s2
            implicitHeight: warning.implicitHeight + Theme.s3 * 2
            radius: Theme.radiusControl
            color: Theme.withAlpha(Theme.warning, 0.08)
            border.color: Theme.withAlpha(Theme.warning, 0.3)
            Text {
                id: warning
                anchors.fill: parent
                anchors.margins: Theme.s3
                wrapMode: Text.WordWrap
                color: Theme.warning
                font.pixelSize: Theme.fontSm
                text: qsTr("YouTube may flag accounts used for downloading. A secondary Google account is safest.")
            }
        }

        RowLayout {
            Layout.topMargin: Theme.s2
            SButton {
                visible: dialog.browser !== "" || dialog.cookiesFile !== ""
                variant: "danger"
                text: qsTr("Stop using sign-in")
                onClicked: { dialog.browser = ""; dialog.cookiesFile = ""; dialog.cleared(); dialog.close() }
            }
            Item { Layout.fillWidth: true }
            SButton { text: qsTr("Cancel"); onClicked: dialog.close() }
            SButton {
                variant: "primary"
                enabled: dialog.browser !== "" || dialog.cookiesFile !== ""
                text: qsTr("I've signed in, retry")
                onClicked: { dialog.applied(dialog.browser, dialog.cookiesFile); dialog.close() }
            }
        }
    }
}
