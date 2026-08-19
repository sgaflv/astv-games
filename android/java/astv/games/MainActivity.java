package astv.games;

import javax.microedition.khronos.egl.EGLConfig;
import javax.microedition.khronos.opengles.GL10;

import android.app.Activity;
import android.os.Bundle;
import android.os.Build;
import android.util.Log;

import android.view.View;
import android.view.Surface;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowManager.LayoutParams;
import android.view.SurfaceView;
import android.view.SurfaceHolder;
import android.view.MotionEvent;
import android.view.KeyEvent;
import android.view.InputDevice;
import android.view.inputmethod.InputMethodManager;

import android.content.Context;
import android.content.Intent;
import android.content.res.Configuration;
import android.content.ClipData;
import android.content.ClipboardManager;

import android.graphics.Color;
import android.graphics.Insets;
import android.view.inputmethod.InputConnection;
import android.view.inputmethod.EditorInfo;
import android.widget.LinearLayout;

import java.util.HashMap;
import java.util.Map;

import quad_native.QuadNative;

class QuadSurface
    extends
        SurfaceView
    implements
        View.OnTouchListener,
        View.OnKeyListener,
        SurfaceHolder.Callback {

    public QuadSurface(Context context){
        super(context);
        getHolder().addCallback(this);

        setFocusable(true);
        setFocusableInTouchMode(true);
        requestFocus();
        setOnTouchListener(this);
        setOnKeyListener(this);
    }

    @Override
    public void surfaceCreated(SurfaceHolder holder) {
        Log.i("SAPP", "surfaceCreated");
        Surface surface = holder.getSurface();
        QuadNative.surfaceOnSurfaceCreated(surface);
    }

    @Override
    public void surfaceDestroyed(SurfaceHolder holder) {
        Log.i("SAPP", "surfaceDestroyed");
        Surface surface = holder.getSurface();
        QuadNative.surfaceOnSurfaceDestroyed(surface);
    }

    @Override
    public void surfaceChanged(SurfaceHolder holder,
                               int format,
                               int width,
                               int height) {
        Log.i("SAPP", "surfaceChanged");
        Surface surface = holder.getSurface();
        QuadNative.surfaceOnSurfaceChanged(surface, width, height);

    }

    @Override
    public boolean onTouch(View v, MotionEvent event) {
        int pointerCount = event.getPointerCount();
        int action = event.getActionMasked();

        switch(action) {
        case MotionEvent.ACTION_MOVE: {
            for (int i = 0; i < pointerCount; i++) {
                final int id = event.getPointerId(i);
                final float x = event.getX(i);
                final float y = event.getY(i);
                QuadNative.surfaceOnTouch(id, 0, x, y);
            }
            break;
        }
        case MotionEvent.ACTION_UP: {
            final int id = event.getPointerId(0);
            final float x = event.getX(0);
            final float y = event.getY(0);
            QuadNative.surfaceOnTouch(id, 1, x, y);
            break;
        }
        case MotionEvent.ACTION_DOWN: {
            final int id = event.getPointerId(0);
            final float x = event.getX(0);
            final float y = event.getY(0);
            QuadNative.surfaceOnTouch(id, 2, x, y);
            break;
        }
        case MotionEvent.ACTION_POINTER_UP: {
            final int pointerIndex = event.getActionIndex();
            final int id = event.getPointerId(pointerIndex);
            final float x = event.getX(pointerIndex);
            final float y = event.getY(pointerIndex);
            QuadNative.surfaceOnTouch(id, 1, x, y);
            break;
        }
        case MotionEvent.ACTION_POINTER_DOWN: {
            final int pointerIndex = event.getActionIndex();
            final int id = event.getPointerId(pointerIndex);
            final float x = event.getX(pointerIndex);
            final float y = event.getY(pointerIndex);
            QuadNative.surfaceOnTouch(id, 2, x, y);
            break;
        }
        case MotionEvent.ACTION_CANCEL: {
            for (int i = 0; i < pointerCount; i++) {
                final int id = event.getPointerId(i);
                final float x = event.getX(i);
                final float y = event.getY(i);
                QuadNative.surfaceOnTouch(id, 3, x, y);
            }
            break;
        }
        default:
            break;
        }

        return true;
    }

    // Gamepad -> player slot assignment. The first gamepad (device with an
    // analog joystick) is player 0 (snake 1), the second is player 1 (snake 2);
    // any extra gamepads share player 1. Non-gamepad devices (TV remote,
    // keyboard) return -1 and keep using the legacy miniquad key path.
    private final Map<Integer, Integer> devicePlayers = new HashMap<Integer, Integer>();
    private int nextPlayer = 0;

    private int playerForDevice(int deviceId) {
        Integer assigned = devicePlayers.get(deviceId);
        if (assigned != null) {
            return assigned;
        }
        InputDevice device = InputDevice.getDevice(deviceId);
    // A device is a gamepad when it exposes analog sticks (SOURCE_JOYSTICK)
    // or gamepad face buttons (SOURCE_GAMEPAD). The D-pad alone
    // (SOURCE_DPAD, shared with TV remotes) is not enough, so D-pad-only
    // gamepads without face buttons still fall back to the legacy path.
    boolean isGamepad = device != null
        && (device.getSources() & (InputDevice.SOURCE_JOYSTICK | InputDevice.SOURCE_GAMEPAD)) != 0;
        if (!isGamepad) {
            return -1;
        }
        int player = Math.min(nextPlayer, 1);
        devicePlayers.put(deviceId, player);
        if (nextPlayer < 1) {
            nextPlayer++;
        }
        return player;
    }

    // Axis indices forwarded to the engine (must match AXIS_COUNT in input.rs):
    // 0 = X, 1 = Y (left stick), 2 = hat X, 3 = hat Y (physical D-pad).
    private static final int AXIS_X = 0;
    private static final int AXIS_Y = 1;
    private static final int AXIS_HAT_X = 2;
    private static final int AXIS_HAT_Y = 3;
    private static final int AXIS_RX = 4;
    private static final int AXIS_RY = 5;

    // Direction synthesis from analog input. Many gamepads report the D-pad as
    // a hat switch and never emit KEYCODE_DPAD_* key events, so directions are
    // derived here from the axes. The stick and the D-pad stay fully separate:
    // the hat produces the standard D-pad keycodes (UP/DOWN/LEFT/RIGHT), while
    // the stick produces synthetic codes 200-203 that the engine maps to its
    // own StickUp/StickDown/StickLeft/StickRight inputs, so games can tell the
    // two apart. Hysteresis keeps a slightly-off-center stick quiet; the hat is
    // usually binary, so its band is tighter. Real D-pad key events still pass
    // through onKey untouched.
    private static final float STICK_PRESS = 0.55f;
    private static final float STICK_RELEASE = 0.35f;
    private static final float HAT_PRESS = 0.5f;
    private static final float HAT_RELEASE = 0.25f;
    // Synthetic stick-direction keycodes (must match the 200-203 arm of
    // android_keycode_to_input in engine/src/input.rs). 200-203 are unassigned
    // in android.view.KeyEvent.
    private static final int STICK_UP = 200;
    private static final int STICK_DOWN = 201;
    private static final int STICK_LEFT = 202;
    private static final int STICK_RIGHT = 203;
    // Per player, per direction slot (0 = UP, 1 = DOWN, 2 = LEFT, 3 = RIGHT):
    // whether a direction is currently synthesized from that source.
    private final boolean[][] stickHeld = new boolean[2][4];
    private final boolean[][] dpadHeld = new boolean[2][4];

    // Emit one direction edge for a player, if the state actually changed.
    private void setDirection(boolean[][] held, int player, int keyCode, int dir, boolean down) {
        if (player < 0 || player >= held.length) {
            return;
        }
        if (dir < 0 || dir >= held[player].length) {
            return;
        }
        if (held[player][dir] == down) {
            return;
        }
        held[player][dir] = down;
        QuadNative.surfaceOnPlayerKey(player, keyCode, down ? 1 : 0);
    }

    // Move one direction's state from an axis value: press past `press`,
    // release below `release` (hysteresis in between keeps the previous state).
    private void updateDirection(boolean[][] held, int player, int keyCode, int dir, float value, float press, float release) {
        if (player < 0 || player >= held.length) {
            return;
        }
        if (dir < 0 || dir >= held[player].length) {
            return;
        }
        if (!held[player][dir] && value >= press) {
            setDirection(held, player, keyCode, dir, true);
        } else if (held[player][dir] && value < release) {
            setDirection(held, player, keyCode, dir, false);
        }
    }

    // Gamepad analog input arrives as generic motion events (one ACTION_MOVE
    // per axis change while the stick or hat moves). Derive directions from
    // the axes and forward the raw values to the engine so the keys tool can
    // display them as percentages.
    @Override
    public boolean onGenericMotionEvent(MotionEvent event) {
        if ((event.getSource() & InputDevice.SOURCE_JOYSTICK) == 0) {
            return super.onGenericMotionEvent(event);
        }
        int player = playerForDevice(event.getDeviceId());
        if (player < 0) {
            return super.onGenericMotionEvent(event);
        }
        float x = event.getAxisValue(MotionEvent.AXIS_X);
        float y = event.getAxisValue(MotionEvent.AXIS_Y);
        float hx = event.getAxisValue(MotionEvent.AXIS_HAT_X);
        float hy = event.getAxisValue(MotionEvent.AXIS_HAT_Y);
        // Right stick (AXIS_Z / AXIS_RZ) on most gamepads.
        float rx = event.getAxisValue(MotionEvent.AXIS_Z);
        float ry = event.getAxisValue(MotionEvent.AXIS_RZ);
        QuadNative.surfaceOnPlayerAxis(player, AXIS_X, x);
        QuadNative.surfaceOnPlayerAxis(player, AXIS_Y, y);
        QuadNative.surfaceOnPlayerAxis(player, AXIS_HAT_X, hx);
        QuadNative.surfaceOnPlayerAxis(player, AXIS_HAT_Y, hy);
        QuadNative.surfaceOnPlayerAxis(player, AXIS_RX, rx);
        QuadNative.surfaceOnPlayerAxis(player, AXIS_RY, ry);

        // Left stick -> stick directions (Up is negative Y, right is positive
        // X). Distinct keycodes, so the engine sees StickUp/StickDown/...
        updateDirection(stickHeld, player, STICK_UP, 0, -y, STICK_PRESS, STICK_RELEASE);
        updateDirection(stickHeld, player, STICK_DOWN, 1, y, STICK_PRESS, STICK_RELEASE);
        updateDirection(stickHeld, player, STICK_LEFT, 2, -x, STICK_PRESS, STICK_RELEASE);
        updateDirection(stickHeld, player, STICK_RIGHT, 3, x, STICK_PRESS, STICK_RELEASE);
        // Physical D-pad hat -> D-pad directions.
        updateDirection(dpadHeld, player, KeyEvent.KEYCODE_DPAD_UP, 0, -hy, HAT_PRESS, HAT_RELEASE);
        updateDirection(dpadHeld, player, KeyEvent.KEYCODE_DPAD_DOWN, 1, hy, HAT_PRESS, HAT_RELEASE);
        updateDirection(dpadHeld, player, KeyEvent.KEYCODE_DPAD_LEFT, 2, -hx, HAT_PRESS, HAT_RELEASE);
        updateDirection(dpadHeld, player, KeyEvent.KEYCODE_DPAD_RIGHT, 3, hx, HAT_PRESS, HAT_RELEASE);
        return true;
    }

    // docs says getCharacters are deprecated
    // but somehow on non-latyn input all keyCode and all the relevant fields in the KeyEvent are zeros
    // and only getCharacters has some usefull data
    @SuppressWarnings("deprecation")
    @Override
    public boolean onKey(View v, int keyCode, KeyEvent event) {
        int player = playerForDevice(event.getDeviceId());
        int action = event.getAction();

        if (player >= 0) {
            // Gamepad device: route by player slot through the device-aware
            // native path. The Rust side translates the raw keycode itself, so
            // the F1-F4 remap below is not needed for gamepads.
            if (action == KeyEvent.ACTION_DOWN) {
                QuadNative.surfaceOnPlayerKey(player, keyCode, 1);
            } else if (action == KeyEvent.ACTION_UP) {
                QuadNative.surfaceOnPlayerKey(player, keyCode, 0);
            }
            return true;
        }

        // Non-gamepad device (TV remote / keyboard): existing miniquad key
        // path, which controls player 0. Android reports gamepad face buttons
        // as KEYCODE_BUTTON_A/B/X/Y (96/97/99/100), which miniquad 0.4.11
        // collapses into KeyCode::Unknown. Remap them onto the unused F1-F4
        // keycodes so the game sees distinct keys (A->F1, B->F2, X->F3, Y->F4).
        // Placeholder: only A and B are used so far; X/Y are preserved for later.
        int code = keyCode;
        switch (keyCode) {
        case KeyEvent.KEYCODE_BUTTON_A: code = KeyEvent.KEYCODE_F1; break;
        case KeyEvent.KEYCODE_BUTTON_B: code = KeyEvent.KEYCODE_F2; break;
        case KeyEvent.KEYCODE_BUTTON_X: code = KeyEvent.KEYCODE_F3; break;
        case KeyEvent.KEYCODE_BUTTON_Y: code = KeyEvent.KEYCODE_F4; break;
        default: break;
        }

        if (event.getAction() == KeyEvent.ACTION_DOWN && code != 0) {
            QuadNative.surfaceOnKeyDown(code);
        }

        if (event.getAction() == KeyEvent.ACTION_UP && code != 0) {
            QuadNative.surfaceOnKeyUp(code);
        }

        if (event.getAction() == KeyEvent.ACTION_UP || event.getAction() == KeyEvent.ACTION_MULTIPLE) {
            int character = event.getUnicodeChar();
            if (character == 0) {
                String characters = event.getCharacters();
                if (characters != null && !characters.isEmpty()) {
                    character = characters.charAt(0);
                }
            }

            if (character != 0) {
                QuadNative.surfaceOnCharacter(character);
            }
        }

        return true;
    }

    // There is an Android bug when screen is in landscape,
    // the keyboard inset height is reported as 0.
    // This code is a workaround which fixes the bug.
    // See https://groups.google.com/g/android-developers/c/50XcWooqk7I
    // For some reason it only works if placed here and not in the parent layout.
    @Override
    public InputConnection onCreateInputConnection(EditorInfo outAttrs) {
        InputConnection connection = super.onCreateInputConnection(outAttrs);
        outAttrs.imeOptions |= EditorInfo.IME_FLAG_NO_FULLSCREEN;
        return connection;
    }

    public Surface getNativeSurface() {
        return getHolder().getSurface();
    }
}

class ResizingLayout
    extends
        LinearLayout
    implements
        View.OnApplyWindowInsetsListener {

    public ResizingLayout(MainActivity activity){
        super(activity);
        // When viewing in landscape mode with keyboard shown, there are
        // gaps on both sides so we fill the negative space with black.
        setBackgroundColor(Color.BLACK);
        setOnApplyWindowInsetsListener(this);
    }

    @Override
    public WindowInsets onApplyWindowInsets(View v, WindowInsets insets) {
        if (Build.VERSION.SDK_INT >= 30) {
            Insets imeInsets = insets.getInsets(WindowInsets.Type.ime());
            Insets sysInsets = insets.getInsets(WindowInsets.Type.systemBars());

            // When IME is visible then we dont need bottom inset
            int bottomPadding = sysInsets.bottom;
            if (imeInsets.bottom > 0) {
                bottomPadding = imeInsets.bottom;
            }

            // The sys insets change when orientation changes and sys bars
            // change position.
            v.setPadding(
                sysInsets.left,
                sysInsets.top,
                sysInsets.right,
                bottomPadding
            );
        }
        return insets;
    }
}

public class MainActivity extends Activity {

    private QuadSurface view;

    static {
        System.loadLibrary("app");
    }

    @Override
    public void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        this.requestWindowFeature(Window.FEATURE_NO_TITLE);

        view = new QuadSurface(this);
        // Put it inside a parent layout which can resize it using padding
        ResizingLayout layout = new ResizingLayout(this);
        layout.addView(view);
        setContentView(layout);

        QuadNative.activityOnCreate(this);
    }

    @Override
    protected void onResume() {
        super.onResume();
        QuadNative.activityOnResume();
    }

    @Override
    public void onBackPressed() {
        Log.w("SAPP", "onBackPressed");

        // TODO: here is the place to handle request_quit/order_quit/cancel_quit

        super.onBackPressed();
    }

    @Override
    protected void onStop() {
        super.onStop();
    }

    @Override
    protected void onDestroy() {
        super.onDestroy();

        QuadNative.activityOnDestroy();
    }

    @Override
    protected void onPause() {
        super.onPause();
        QuadNative.activityOnPause();
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
    }

    public void setFullScreen(final boolean fullscreen) {
        runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    View decorView = getWindow().getDecorView();

                    if (fullscreen) {
                        getWindow().setFlags(LayoutParams.FLAG_LAYOUT_NO_LIMITS, LayoutParams.FLAG_LAYOUT_NO_LIMITS);
                        if (Build.VERSION.SDK_INT >= 28) {
                            getWindow().getAttributes().layoutInDisplayCutoutMode = LayoutParams.LAYOUT_IN_DISPLAY_CUTOUT_MODE_SHORT_EDGES;
                        }
                        if (Build.VERSION.SDK_INT >= 30) {
                            getWindow().setDecorFitsSystemWindows(false);
                        } else {
                            int uiOptions = View.SYSTEM_UI_FLAG_HIDE_NAVIGATION | View.SYSTEM_UI_FLAG_FULLSCREEN | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY;
                            decorView.setSystemUiVisibility(uiOptions);
                        }
                    }
                    else {
                        if (Build.VERSION.SDK_INT >= 30) {
                            getWindow().setDecorFitsSystemWindows(true);
                        } else {
                          decorView.setSystemUiVisibility(0);
                        }

                    }
                }
            });
    }

    public void showKeyboard(final boolean show) {
        runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    if (show) {
                        InputMethodManager imm = (InputMethodManager)getSystemService(Context.INPUT_METHOD_SERVICE);
                        imm.showSoftInput(view, 0);
                    } else {
                        InputMethodManager imm = (InputMethodManager) getSystemService(Context.INPUT_METHOD_SERVICE);
                        imm.hideSoftInputFromWindow(view.getWindowToken(),0);
                    }
                }
            });
    }

    public String getClipboardText() {
        ClipboardManager clipboard = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);

        if (!clipboard.hasPrimaryClip())
            return null;

        ClipData primaryClip = clipboard.getPrimaryClip();
        if (primaryClip == null || primaryClip.getItemCount() < 1)
            return null;

        CharSequence clipData = clipboard.getPrimaryClip().getItemAt(0).getText();
        if (clipData == null) {
            return null;
        }

        return clipData.toString();
    }
    public void setClipboardText(String text) {
        ClipboardManager clipboard = (ClipboardManager) getSystemService(Context.CLIPBOARD_SERVICE);
        ClipData clip = ClipData.newPlainText("label", text);
        clipboard.setPrimaryClip(clip);
    }
}
