package app.invoicing.android.data

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Stores the server URL and the login session. The session token is encrypted with an AES-GCM key that lives
 * in the Android Keystore, so it cannot be read from a backup or by copying the app's files.
 */
class SecureSettings(context: Context) {
    private val prefs = context.getSharedPreferences("connection", Context.MODE_PRIVATE)

    var baseUrl: String
        get() = prefs.getString(KEY_URL, DEFAULT_URL) ?: DEFAULT_URL
        set(value) = prefs.edit().putString(KEY_URL, value.trim()).apply()

    /** The session token of the signed-in user ("" when signed out or expired). */
    var token: String
        get() = prefs.getString(KEY_TOKEN, null)?.let { runCatching { decrypt(it) }.getOrNull() } ?: ""
        private set(value) = prefs.edit().putString(KEY_TOKEN, encrypt(value)).apply()

    /** The signed-in user, or `null` if there is no valid session. */
    fun currentUser(): UserInfo? {
        if (token.isEmpty() || prefs.getLong(KEY_EXPIRES, 0) * 1000 <= System.currentTimeMillis()) return null
        val name = prefs.getString(KEY_USER, null) ?: return null
        return UserInfo(prefs.getLong(KEY_USER_ID, 0), name, prefs.getString(KEY_ROLE, "operator") ?: "operator")
    }

    fun saveSession(login: LoginResult) {
        token = login.token
        prefs.edit()
            .putLong(KEY_EXPIRES, login.expiresAt)
            .putLong(KEY_USER_ID, login.user.id)
            .putString(KEY_USER, login.user.username)
            .putString(KEY_ROLE, login.user.role)
            .apply()
    }

    fun clearSession() {
        prefs.edit().remove(KEY_TOKEN).remove(KEY_EXPIRES).remove(KEY_USER).remove(KEY_USER_ID).remove(KEY_ROLE).apply()
    }

    fun connection() = ApiClient.Connection(baseUrl, token)

    private fun secretKey(): SecretKey {
        val store = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (store.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        generator.init(
            KeyGenParameterSpec.Builder(KEY_ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .build(),
        )
        return generator.generateKey()
    }

    private fun encrypt(plain: String): String {
        val cipher = Cipher.getInstance(TRANSFORMATION).apply { init(Cipher.ENCRYPT_MODE, secretKey()) }
        val encrypted = cipher.doFinal(plain.toByteArray())
        return Base64.encodeToString(cipher.iv + encrypted, Base64.NO_WRAP)
    }

    private fun decrypt(encoded: String): String {
        val bytes = Base64.decode(encoded, Base64.NO_WRAP)
        val cipher = Cipher.getInstance(TRANSFORMATION).apply {
            init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(128, bytes, 0, IV_SIZE))
        }
        return String(cipher.doFinal(bytes, IV_SIZE, bytes.size - IV_SIZE))
    }

    private companion object {
        const val KEY_URL = "base_url"
        const val KEY_TOKEN = "session_token_encrypted"
        const val KEY_EXPIRES = "session_expires_at"
        const val KEY_USER = "user_name"
        const val KEY_USER_ID = "user_id"
        const val KEY_ROLE = "user_role"
        const val KEY_ALIAS = "invoicing_session_token"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_SIZE = 12

        /** The emulator reaches the host machine at 10.0.2.2. */
        const val DEFAULT_URL = "http://10.0.2.2:3000"
    }
}
