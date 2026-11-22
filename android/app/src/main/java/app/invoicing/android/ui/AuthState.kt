package app.invoicing.android.ui

import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import app.invoicing.android.data.LoginResult
import app.invoicing.android.data.SecureSettings
import app.invoicing.android.data.UserInfo

/** Who is signed in. The screens recompose when it changes, so signing out shows the login. */
class AuthState(private val settings: SecureSettings) {
    var user by mutableStateOf<UserInfo?>(settings.currentUser())
        private set

    fun signedIn(login: LoginResult) {
        settings.saveSession(login)
        user = login.user
    }

    /** The server refused the token (expired or revoked), or the user chose to sign out. */
    fun expire() {
        settings.clearSession()
        user = null
    }
}
