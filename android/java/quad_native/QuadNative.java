package quad_native;

import android.view.Surface;

// Java forces the MainActivity class to belong to a specially named package
// this package name is forced to be different for each app
// and Java does not have any way to specify a native symbol lookup name..
// To workaround this - all native callbacks live in QuadNative class
public class QuadNative {
    // belongs to MainActivity class
    public native static void activityOnCreate(Object activity);
    public native static void activityOnResume();
    public native static void activityOnPause();
    public native static void activityOnDestroy();

    // belongs to QuadSurface class
    public native static void surfaceOnSurfaceCreated(Surface surface);
    public native static void surfaceOnSurfaceDestroyed(Surface surface);
    public native static void surfaceOnTouch(int id, int phase, float x, float y);
    public native static void surfaceOnSurfaceChanged(Surface surface, int width, int height);
    public native static void surfaceOnKeyDown(int keycode);
    public native static void surfaceOnKeyUp(int keycode);
    public native static void surfaceOnCharacter(int character);
    // Device-aware gamepad input: player slot (0/1) + raw keycode + down flag.
    // Implemented in the engine crate (input.rs) because miniquad 0.4 cannot
    // distinguish gamepads.
    public native static void surfaceOnPlayerKey(int player, int keycode, int down);
    // Live analog stick axis (0 = X, 1 = Y) for a player slot, in -1..1.
    public native static void surfaceOnPlayerAxis(int player, int axis, float value);
}
