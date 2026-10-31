package org.gbadn.console;

import android.app.NativeActivity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.view.View;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;

/**
 * Hosts the Rust console and lends it Android's file picker.
 *
 * A picked document has no folder the console could keep saves in, so accepted
 * cartridges are copied to private storage. Rust then finds the path in
 * pending_cartridge. Refused names are never copied: the path points at nothing,
 * and the console's own check explains the refusal.
 */
public class GbadnActivity extends NativeActivity {
    private static final int PICK_CARTRIDGE = 1;
    private static final String[] ACCEPTED_SUFFIXES = {
        ".fasta", ".fa", ".fna", ".fasta.gz", ".fa.gz", ".fna.gz",
    };

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        importFromViewIntent(getIntent());
    }

    @Override
    protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        importFromViewIntent(intent);
    }

    @Override
    public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) {
            hideSystemBars();
        }
    }

    /** Called from Rust, on a thread that isn't the UI's. */
    public void pickCartridge() {
        runOnUiThread(() -> {
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType("*/*");
            startActivityForResult(intent, PICK_CARTRIDGE);
        });
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode == PICK_CARTRIDGE && resultCode == RESULT_OK && data != null) {
            importCartridge(data.getData());
        }
    }

    @SuppressWarnings("deprecation")
    private void hideSystemBars() {
        getWindow().getDecorView().setSystemUiVisibility(
            View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
    }

    private void importFromViewIntent(Intent intent) {
        if (intent != null && Intent.ACTION_VIEW.equals(intent.getAction())) {
            importCartridge(intent.getData());
        }
    }

    private void importCartridge(Uri uri) {
        if (uri == null) {
            return;
        }
        new Thread(() -> {
            File cartridges = new File(getFilesDir(), "cartridges");
            cartridges.mkdirs();
            File target = new File(cartridges, displayName(uri));
            try {
                if (isAccepted(target.getName())) {
                    copy(uri, target);
                }
                announce(target);
            } catch (IOException e) {
                target.delete();
                try {
                    announce(target);
                } catch (IOException ignored) {
                    // With storage this broken, there is no way left to tell the console.
                }
            }
        }).start();
    }

    /** The document's name, reduced to a bare file name. */
    private String displayName(Uri uri) {
        String name = null;
        try (Cursor cursor = getContentResolver().query(
                uri, new String[] {OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                name = cursor.getString(0);
            }
        }
        if (name == null) {
            name = uri.getLastPathSegment();
        }
        name = name == null ? "" : new File(name).getName();
        return name.isEmpty() ? "cartridge" : name;
    }

    private static boolean isAccepted(String name) {
        String lower = name.toLowerCase(java.util.Locale.ROOT);
        for (String suffix : ACCEPTED_SUFFIXES) {
            if (lower.endsWith(suffix)) {
                return true;
            }
        }
        return false;
    }

    private void copy(Uri uri, File target) throws IOException {
        File partial = new File(target.getPath() + ".partial");
        try (InputStream in = getContentResolver().openInputStream(uri);
                OutputStream out = new FileOutputStream(partial)) {
            if (in == null) {
                throw new IOException("the document could not be opened");
            }
            byte[] buffer = new byte[64 * 1024];
            int count;
            while ((count = in.read(buffer)) > 0) {
                out.write(buffer, 0, count);
            }
        } catch (IOException e) {
            partial.delete();
            throw e;
        }
        if (!partial.renameTo(target)) {
            partial.delete();
            throw new IOException("the cartridge could not be stored");
        }
    }

    private void announce(File cartridge) throws IOException {
        File partial = new File(getFilesDir(), "pending_cartridge.partial");
        try (OutputStream out = new FileOutputStream(partial)) {
            out.write(cartridge.getPath().getBytes(StandardCharsets.UTF_8));
        }
        if (!partial.renameTo(new File(getFilesDir(), "pending_cartridge"))) {
            throw new IOException("the cartridge could not be announced");
        }
    }
}
