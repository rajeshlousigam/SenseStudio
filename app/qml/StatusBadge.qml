import QtQuick
import SenseStudio

// Pill per TMS convention: 15% fill, 30% border, solid text.
Rectangle {
    id: badge
    required property int status
    required property string text

    readonly property color tone: {
        switch (status) {
        case MediaLibrary.Ready:
        case MediaLibrary.Converted: return Theme.success
        case MediaLibrary.NeedsRemux: return Theme.info
        case MediaLibrary.NeedsAudio: return Theme.warning
        case MediaLibrary.NeedsTranscode: return Theme.orange
        case MediaLibrary.Converting: return Theme.accentLight
        case MediaLibrary.Failed: return Theme.danger
        default: return Theme.textSecondary
        }
    }

    implicitWidth: label.implicitWidth + 16
    implicitHeight: 20
    radius: 6
    color: Theme.withAlpha(tone, 0.15)
    border.width: 1
    border.color: Theme.withAlpha(tone, 0.3)

    Text {
        id: label
        anchors.centerIn: parent
        text: badge.text
        color: badge.tone
        font.pixelSize: Theme.fontXs
        font.weight: Font.DemiBold
    }
}
