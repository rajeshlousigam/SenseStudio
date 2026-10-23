#include "ThumbnailProvider.h"

#include <QFileInfo>
#include <QFontDatabase>
#include <QGuiApplication>
#include <QQmlApplicationEngine>
#include <QQuickStyle>
#include <QQuickWindow>
#include <QTimer>

int main(int argc, char *argv[])
{
    QGuiApplication app(argc, argv);
    QGuiApplication::setOrganizationName(QStringLiteral("StepOut"));
    QGuiApplication::setOrganizationDomain(QStringLiteral("stepout.ai"));
    QGuiApplication::setApplicationName(QStringLiteral("SenseStudio"));
    QGuiApplication::setApplicationVersion(QStringLiteral(SENSE_VERSION));

    // Basic is the lightest Controls style and is fully re-skinned in QML.
    QQuickStyle::setStyle(QStringLiteral("Basic"));

    // StepOut's UI face; falls back to the platform font when it is not installed.
    if (QFontDatabase::families().contains(QStringLiteral("Inter"))) {
        QFont font(QStringLiteral("Inter"));
        font.setPixelSize(13);
        QGuiApplication::setFont(font);
    }

    // Positional args are files/folders or http(s) links to import ("Open with…"); --view <0|1|2> picks the start
    // view and --output-dir <dir> overrides the output folder for this session only. Dev flags for
    // headless smoke tests: --convert starts the queue once analysis finishes; --download <url>
    // queues a download and --merge merges every downloaded video once downloads finish; --screenshot <png>
    // saves the window after --delay <ms> (default 4000) and quits.
    QList<QUrl> initialUrls;
    QStringList initialLinks;
    QStringList initialDownloads;
    bool autoMerge = false;
    QString screenshotPath;
    int screenshotDelay = 4000;
    bool autoConvert = false;
    QString outputOverride;
    int initialView = 0;
    const QStringList args = QGuiApplication::arguments().mid(1);
    for (qsizetype i = 0; i < args.size(); ++i) {
        const QString &arg = args.at(i);
        if (arg == QLatin1String("--screenshot") && i + 1 < args.size())
            screenshotPath = args.at(++i);
        else if (arg == QLatin1String("--delay") && i + 1 < args.size())
            screenshotDelay = args.at(++i).toInt();
        else if (arg == QLatin1String("--convert"))
            autoConvert = true;
        else if (arg == QLatin1String("--output-dir") && i + 1 < args.size())
            outputOverride = QFileInfo(args.at(++i)).absoluteFilePath();
        else if (arg == QLatin1String("--view") && i + 1 < args.size())
            initialView = args.at(++i).toInt();
        else if (arg == QLatin1String("--download") && i + 1 < args.size())
            initialDownloads << args.at(++i);
        else if (arg == QLatin1String("--merge"))
            autoMerge = true;
        else if (arg.startsWith(QLatin1String("http://")) || arg.startsWith(QLatin1String("https://")))
            initialLinks << arg;
        else if (!arg.startsWith(QLatin1Char('-')))
            initialUrls << QUrl::fromLocalFile(QFileInfo(arg).absoluteFilePath());
    }

    QQmlApplicationEngine engine;
    engine.setInitialProperties({
        {QStringLiteral("initialUrls"), QVariant::fromValue(initialUrls)},
        {QStringLiteral("initialLinks"), initialLinks.join(QLatin1Char('\n'))},
        {QStringLiteral("initialDownloads"), initialDownloads.join(QLatin1Char('\n'))},
        {QStringLiteral("autoMerge"), autoMerge},
        {QStringLiteral("autoConvert"), autoConvert},
        {QStringLiteral("outputOverride"), outputOverride},
        {QStringLiteral("view"), initialView},
    });
    engine.addImageProvider(QStringLiteral("thumbs"), new ThumbnailProvider);
    QObject::connect(&engine, &QQmlApplicationEngine::objectCreationFailed, &app,
                     [] { QCoreApplication::exit(1); }, Qt::QueuedConnection);
    engine.loadFromModule("SenseStudio", "Main");

    if (!screenshotPath.isEmpty()) {
        QTimer::singleShot(screenshotDelay, &app, [&engine, screenshotPath] {
            auto *window = engine.rootObjects().isEmpty() ? nullptr : qobject_cast<QQuickWindow *>(engine.rootObjects().constFirst());
            const bool saved = window && window->grabWindow().save(screenshotPath);
            QCoreApplication::exit(saved ? 0 : 2);
        });
    }
    return QGuiApplication::exec();
}
