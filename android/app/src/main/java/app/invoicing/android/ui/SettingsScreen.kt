package app.invoicing.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import kotlinx.coroutines.launch
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.SecureSettings

/**
 * Account and server. The issuer details and the certificate are managed from the web or desktop
 * app; here the status is only shown, so the .p12 never travels through the phone.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(api: ApiClient, settings: SecureSettings, auth: AuthState) {
    var url by remember { mutableStateOf(settings.baseUrl) }
    var saved by remember { mutableStateOf(false) }
    var version by remember { mutableIntStateOf(0) }
    val config = rememberRemote(version) { api.config() }
    val scope = rememberCoroutineScope()
    val user = auth.user

    Scaffold(topBar = { TopAppBar(title = { Text("Settings") }) }) { padding ->
        Column(Modifier.padding(padding).padding(16.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(14.dp)) {
            Card { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("Account", style = MaterialTheme.typography.titleMedium)
                LabeledValue("Signed in as", user?.username)
                LabeledValue("Role", if (user?.isAdmin == true) "Administrator" else "Member")
                Button(onClick = {
                    scope.launch {
                        runCatching { api.logout() }
                        auth.expire()
                    }
                }) { Text("Log out") }
            } }

            Card { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Text("Server", style = MaterialTheme.typography.titleMedium)
                OutlinedTextField(url, { url = it; saved = false }, label = { Text("Server URL") }, singleLine = true, modifier = Modifier.fillMaxWidth())
                Button(onClick = {
                    settings.baseUrl = url
                    saved = true
                    version++
                }) { Text("Save and test") }
                if (saved) Text("Saved.", style = MaterialTheme.typography.bodySmall)
                Text(
                    "Use HTTPS outside the local network. The emulator reaches your computer at http://10.0.2.2:3000.",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            } }

            config.error?.let { ErrorNotice(it) }
            if (config.loading && config.value == null) Loading()
            config.value?.let { c ->
                Card { Column(Modifier.padding(16.dp)) {
                    Text("Server status", style = MaterialTheme.typography.titleMedium)
                    LabeledValue("Environment", if (c.environment == "production") "Production" else "Test")
                    LabeledValue("Issuer", c.issuerName ?: "Not configured")
                    LabeledValue(
                        "Certificate",
                        when {
                            !c.certificateLoaded -> "Not uploaded"
                            c.certificateValid == false -> "Expired"
                            else -> c.certificateSubject ?: "Loaded"
                        },
                    )
                    if (!c.ready) ErrorNotice("Setup is incomplete: load your RUC details and signature (.p12) from the web or desktop app before issuing invoices.", Modifier.padding(top = 8.dp))
                } }
            }
        }
    }
}
