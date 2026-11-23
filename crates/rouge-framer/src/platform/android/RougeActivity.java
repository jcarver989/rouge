package @@ID@@;

import android.content.res.Configuration;
import android.os.Bundle;
import androidx.core.view.WindowCompat;
import androidx.core.view.WindowInsetsControllerCompat;
import com.google.androidgamesdk.GameActivity;
import com.google.androidgamesdk.gametextinput.State;

public class RougeActivity extends GameActivity {
    private static final int MAX_TEXT_BYTES = (1 << 16) - 1;
    private static final String REJECTED = "\0";

    @Override
    public void stateChanged(State state, boolean dismissed) {
        super.stateChanged(fits(state.text) ? state : new State(REJECTED, 0, 0, -1, -1), dismissed);
    }

    private static boolean fits(String text) {
        int bytes = 0;
        for (int index = 0; index < text.length() && bytes <= MAX_TEXT_BYTES; index++) {
            char unit = text.charAt(index);
            bytes += unit != 0 && unit < 0x80 ? 1 : unit < 0x800 ? 2 : 3;
        }
        return bytes <= MAX_TEXT_BYTES;
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        WindowCompat.setDecorFitsSystemWindows(getWindow(), false);
        super.onCreate(savedInstanceState);
        styleSystemBars(getResources().getConfiguration());
    }

    @Override
    public void onConfigurationChanged(Configuration configuration) {
        super.onConfigurationChanged(configuration);
        styleSystemBars(configuration);
    }

    private void styleSystemBars(Configuration configuration) {
        boolean light = (configuration.uiMode & Configuration.UI_MODE_NIGHT_MASK) != Configuration.UI_MODE_NIGHT_YES;
        WindowInsetsControllerCompat bars = WindowCompat.getInsetsController(getWindow(), getWindow().getDecorView());
        bars.setAppearanceLightStatusBars(light);
        bars.setAppearanceLightNavigationBars(light);
    }
}
