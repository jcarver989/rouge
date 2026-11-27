package rouge.webview;

import android.app.Activity;
import android.graphics.Bitmap;
import android.graphics.Outline;
import android.net.Uri;
import android.util.Log;
import android.view.MotionEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.ViewOutlineProvider;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceError;
import android.webkit.WebResourceRequest;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import androidx.webkit.JavaScriptReplyProxy;
import androidx.webkit.WebMessageCompat;
import androidx.webkit.WebViewCompat;
import androidx.webkit.WebViewFeature;
import java.util.Collections;
import java.util.Set;

public final class WebPage {
    private static final String BRIDGE_NAME = "rougeBridge";
    private static final Set<String> ANY_ORIGIN = Collections.singleton("*");
    private static final char HELLO = 'h';
    private static final char MESSAGE = 'm';
    private static final String BRIDGE = "(function(){"
            + "if(window!==window.top||window.__rougeBridge)return;window.__rougeBridge=true;"
            + "var bridge=window.rougeBridge;"
            + "bridge.onmessage=function(event){"
            + "window.dispatchEvent(new MessageEvent('message',{data:event.data}));};"
            + "window.addEventListener('message',function(event){"
            + "if(event.source!==window)return;"
            + "var data=event.data;"
            + "bridge.postMessage('m'+(typeof data==='string'?data:JSON.stringify(data)));});"
            + "bridge.postMessage('h');})();";

    private final Activity activity;
    private final long peer;
    private final float[] radii = new float[4];
    private FrameLayout clip;
    private WebView web;
    private JavaScriptReplyProxy reply;
    private boolean interactive = true;
    private boolean closed;

    private static native void onLoading(long peer, boolean loading);

    private static native void onProgress(long peer, float progress);

    private static native void onUrl(long peer, String url);

    private static native void onTitle(long peer, String title);

    private static native void onHistory(long peer, boolean back, boolean forward);

    private static native void onFailed(long peer, String reason);

    private static native void onMessage(long peer, String message);

    private static native void onClosed(long peer);

    public WebPage(Activity activity, long peer, boolean inspectable) {
        this.activity = activity;
        this.peer = peer;
        activity.runOnUiThread(() -> open(inspectable));
    }

    public void loadUrl(String url) {
        onUi(() -> web.loadUrl(url));
    }

    public void loadHtml(String html, String baseUrl) {
        onUi(() -> web.loadDataWithBaseURL(baseUrl, html, "text/html", "utf-8", null));
    }

    public void place(boolean interactive, int[] frames, float zoom, float[] radii) {
        onUi(() -> {
            takeInput(interactive);
            clip.setVisibility(View.VISIBLE);
            ViewGroup.LayoutParams outer = clip.getLayoutParams();
            outer.width = Math.max(frames[2], 1);
            outer.height = Math.max(frames[3], 1);
            clip.setLayoutParams(outer);
            clip.setX(frames[0]);
            clip.setY(frames[1]);
            ViewGroup.LayoutParams inner = web.getLayoutParams();
            inner.width = Math.max(frames[6], 1);
            inner.height = Math.max(frames[7], 1);
            web.setLayoutParams(inner);
            web.setX(frames[4]);
            web.setY(frames[5]);
            web.setScaleX(zoom);
            web.setScaleY(zoom);
            System.arraycopy(radii, 0, this.radii, 0, this.radii.length);
            clip.invalidateOutline();
        });
    }

    public void hide(boolean interactive) {
        onUi(() -> {
            takeInput(interactive);
            clip.setVisibility(View.GONE);
        });
    }

    public void reload() {
        onUi(() -> web.reload());
    }

    public void stop() {
        onUi(() -> web.stopLoading());
    }

    public void goBack() {
        onUi(() -> web.goBack());
    }

    public void goForward() {
        onUi(() -> web.goForward());
    }

    public void post(String message) {
        onUi(() -> {
            if (reply != null) {
                reply.postMessage(message);
            }
        });
    }

    public void close() {
        onUi(() -> {
            closed = true;
            ((ViewGroup) clip.getParent()).removeView(clip);
            web.destroy();
            onClosed(peer);
        });
    }

    private void onUi(Runnable action) {
        activity.runOnUiThread(() -> {
            if (!closed) {
                action.run();
            }
        });
    }

    private void takeInput(boolean interactive) {
        this.interactive = interactive;
        if (!interactive) {
            web.clearFocus();
        }
    }

    private void open(boolean inspectable) {
        WebView.setWebContentsDebuggingEnabled(inspectable);
        web = new WebView(activity);
        web.getSettings().setJavaScriptEnabled(true);
        web.getSettings().setDomStorageEnabled(true);
        web.setPivotX(0);
        web.setPivotY(0);
        if (WebViewFeature.isFeatureSupported(WebViewFeature.WEB_MESSAGE_LISTENER)
                && WebViewFeature.isFeatureSupported(WebViewFeature.DOCUMENT_START_SCRIPT)) {
            WebViewCompat.addWebMessageListener(web, BRIDGE_NAME, ANY_ORIGIN, this::heard);
            WebViewCompat.addDocumentStartJavaScript(web, BRIDGE, ANY_ORIGIN);
        } else {
            Log.w("Rouge", "this WebView is too old to exchange messages with its page");
        }
        web.setWebViewClient(new WebViewClient() {
            @Override
            public void onPageStarted(WebView view, String url, Bitmap favicon) {
                if (!closed) {
                    onLoading(peer, true);
                    onUrl(peer, text(url));
                }
            }

            @Override
            public void onPageFinished(WebView view, String url) {
                if (!closed) {
                    onLoading(peer, false);
                    onTitle(peer, text(view.getTitle()));
                    onHistory(peer, view.canGoBack(), view.canGoForward());
                }
            }

            @Override
            public void doUpdateVisitedHistory(WebView view, String url, boolean isReload) {
                if (!closed) {
                    onUrl(peer, text(url));
                    onHistory(peer, view.canGoBack(), view.canGoForward());
                }
            }

            @Override
            public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
                if (!closed && request.isForMainFrame()) {
                    onFailed(peer, String.valueOf(error.getDescription()));
                }
            }
        });
        web.setWebChromeClient(new WebChromeClient() {
            @Override
            public void onProgressChanged(WebView view, int progress) {
                if (!closed) {
                    onProgress(peer, progress / 100f);
                }
            }

            @Override
            public void onReceivedTitle(WebView view, String title) {
                if (!closed) {
                    onTitle(peer, text(title));
                }
            }
        });
        clip = new FrameLayout(activity) {
            @Override
            public boolean dispatchTouchEvent(MotionEvent event) {
                return interactive && super.dispatchTouchEvent(event);
            }
        };
        clip.addView(web, new FrameLayout.LayoutParams(1, 1));
        clip.setClipToOutline(true);
        clip.setOutlineProvider(new ViewOutlineProvider() {
            @Override
            public void getOutline(View view, Outline outline) {
                float radius = Math.max(Math.max(radii[0], radii[1]), Math.max(radii[2], radii[3]));
                int reach = (int) Math.ceil(radius);
                int left = radii[0] <= 0 && radii[3] <= 0 ? -reach : 0;
                int top = radii[0] <= 0 && radii[1] <= 0 ? -reach : 0;
                int right = view.getWidth() + (radii[1] <= 0 && radii[2] <= 0 ? reach : 0);
                int bottom = view.getHeight() + (radii[2] <= 0 && radii[3] <= 0 ? reach : 0);
                outline.setRoundRect(left, top, right, bottom, radius);
            }
        });
        clip.setVisibility(View.GONE);
        activity.addContentView(clip, new FrameLayout.LayoutParams(1, 1));
    }

    private void heard(
            WebView view, WebMessageCompat message, Uri origin, boolean mainFrame, JavaScriptReplyProxy proxy) {
        String data = message.getData();
        if (closed || !mainFrame || data == null || data.isEmpty()) {
            return;
        }
        if (data.charAt(0) == HELLO) {
            reply = proxy;
        } else if (data.charAt(0) == MESSAGE) {
            onMessage(peer, data.substring(1));
        }
    }

    private static String text(String value) {
        return value == null ? "" : value;
    }
}
