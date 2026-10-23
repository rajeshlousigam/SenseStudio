#pragma once

#include <QQuickAsyncImageProvider>
#include <QThreadPool>

// Poster frames for the library list (id "<b64 path>") and timeline
// filmstrips (id "strip/<count>/<b64 path>"). Extraction runs on a small dedicated
// pool (never the GUI thread) and results are cached on disk keyed by
// path + size + mtime, so a reopened library shows thumbnails instantly.
class ThumbnailProvider : public QQuickAsyncImageProvider
{
public:
    ThumbnailProvider();
    QQuickImageResponse *requestImageResponse(const QString &id, const QSize &requestedSize) override;

private:
    QThreadPool m_pool;
};
