pragma Singleton
import QtQuick

// StepOut TMS design tokens (stepouttms/src/theme + SODesignSystem.md), dark-first.
QtObject {
    readonly property color canvas: "#131320"
    readonly property color sidebar: "#0E0E1A"
    readonly property color surface: "#21212E"
    readonly property color surfaceAlt: "#1F2A37"
    readonly property color elevated: "#31313F"
    readonly property color inset: "#171724"
    readonly property color hover: Qt.rgba(1, 1, 1, 0.05)
    readonly property color pressed: Qt.rgba(1, 1, 1, 0.08)

    readonly property color textPrimary: "#FFFFFF"
    readonly property color textSecondary: "#9CA3AF"
    readonly property color textMuted: "#6B7280"

    readonly property color borderSubtle: Qt.rgba(1, 1, 1, 0.07)
    readonly property color border: "#374151"
    readonly property color borderStrong: "#4B5563"
    readonly property color borderAccent: Qt.rgba(14 / 255, 165 / 255, 233 / 255, 0.3)

    readonly property color brand: "#1C64F2"
    readonly property color brandHover: "#1A56DB"
    readonly property color brandSubtle: Qt.rgba(28 / 255, 100 / 255, 242 / 255, 0.12)
    readonly property color accent: "#0EA5E9"
    readonly property color accentLight: "#38BDF8"
    readonly property color ai: "#7C3AED"

    readonly property color success: "#31C48D"
    readonly property color warning: "#FACA15"
    readonly property color orange: "#FB923C"
    readonly property color danger: "#F05252"
    readonly property color info: "#76A9FA"

    readonly property int radiusSm: 4
    readonly property int radiusMd: 8
    readonly property int radiusControl: 10
    readonly property int radiusLg: 12
    readonly property int radiusXl: 16

    readonly property int s1: 4
    readonly property int s2: 8
    readonly property int s3: 12
    readonly property int s4: 16
    readonly property int s5: 20
    readonly property int s6: 24

    readonly property int fontXs: 11
    readonly property int fontSm: 12
    readonly property int fontBase: 13
    readonly property int fontMd: 14
    readonly property int fontLg: 16
    readonly property int fontXl: 20

    readonly property int controlHeight: 34
    readonly property int durationFast: 150
    readonly property string monoFamily: Qt.platform.os === "osx" ? "Menlo" : "Consolas"

    function withAlpha(c, a) { return Qt.rgba(c.r, c.g, c.b, a) }

    function formatDuration(seconds) {
        if (!(seconds >= 0)) return "--:--"
        const s = Math.floor(seconds % 60), m = Math.floor(seconds / 60) % 60, h = Math.floor(seconds / 3600)
        const pad = n => n < 10 ? "0" + n : "" + n
        return (h > 0 ? h + ":" + pad(m) : m) + ":" + pad(s)
    }

    function formatTimecode(ms, fps) {
        if (!(ms >= 0)) return "00:00:00:00"
        const rate = fps > 0 ? Math.round(fps) : 30
        const total = ms / 1000
        const f = Math.floor((total % 1) * rate), s = Math.floor(total) % 60
        const m = Math.floor(total / 60) % 60, h = Math.floor(total / 3600)
        const pad = n => n < 10 ? "0" + n : "" + n
        return pad(h) + ":" + pad(m) + ":" + pad(s) + ":" + pad(f)
    }

    function formatSize(bytes) {
        if (!(bytes > 0)) return ""
        const units = ["B", "KB", "MB", "GB", "TB"]
        let v = bytes, i = 0
        while (v >= 1024 && i < units.length - 1) { v /= 1024; ++i }
        return v.toFixed(i >= 2 ? 1 : 0) + " " + units[i]
    }
}
