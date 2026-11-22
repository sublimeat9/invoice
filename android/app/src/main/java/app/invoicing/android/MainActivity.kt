package app.invoicing.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.SecureSettings
import app.invoicing.android.ui.AuthState
import app.invoicing.android.ui.InvoicingApp
import app.invoicing.android.ui.InvoicingTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        val settings = SecureSettings(applicationContext)
        val auth = AuthState(settings)
        val api = ApiClient(settings::connection, onUnauthorized = auth::expire)
        setContent {
            InvoicingTheme { InvoicingApp(api, settings, auth) }
        }
    }
}
