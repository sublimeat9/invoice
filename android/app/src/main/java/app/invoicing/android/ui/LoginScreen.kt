package app.invoicing.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.SecureSettings
import kotlinx.coroutines.launch

/**
 * Sign in. On the very first run of a new server it creates the administrator instead
 * (the server only allows that while it has no users).
 */
@Composable
fun LoginScreen(api: ApiClient, settings: SecureSettings, auth: AuthState) {
    val scope = rememberCoroutineScope()
    var url by remember { mutableStateOf(settings.baseUrl) }
    var username by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var confirm by remember { mutableStateOf("") }
    var setupRequired by remember { mutableStateOf(false) }
    var checks by remember { mutableIntStateOf(0) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var signingUp by remember { mutableStateOf(false) }
    val creating = setupRequired || signingUp

    // Ask the server whether it needs the first-run setup, now and after "Check connection".
    LaunchedEffect(checks) {
        try {
            setupRequired = api.setupRequired()
            error = null
        } catch (e: Exception) {
            error = e.userMessage()
        }
    }

    fun submit() {
        error = null
        if (creating && password != confirm) { error = "The passwords do not match"; return }
        scope.launch {
            busy = true
            try {
                val login = when {
                    setupRequired -> api.createAdmin(username, password)
                    signingUp -> api.register(username, password)
                    else -> api.login(username, password)
                }
                auth.signedIn(login)
            } catch (e: Exception) {
                error = e.userMessage()
            }
            busy = false
        }
    }

    Scaffold { padding ->
        Column(
            Modifier.padding(padding).fillMaxSize().padding(24.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(if (setupRequired) "Create the administrator" else if (signingUp) "Create an account" else "Sign in", style = MaterialTheme.typography.headlineMedium)
            if (setupRequired) {
                Text(
                    "First run: this account manages settings, the certificate and the other users.",
                    style = MaterialTheme.typography.bodyMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            error?.let { ErrorNotice(it) }

            OutlinedTextField(url, { url = it }, label = { Text("Server URL") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            TextButton(onClick = { settings.baseUrl = url; checks++ }) { Text("Check connection") }

            OutlinedTextField(username, { username = it }, label = { Text("Username") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(password, { password = it }, label = { Text("Password") }, singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password), modifier = Modifier.fillMaxWidth())
            if (creating) {
                OutlinedTextField(confirm, { confirm = it }, label = { Text("Repeat the password") }, singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password), modifier = Modifier.fillMaxWidth())
            }
            Button(
                onClick = { settings.baseUrl = url; submit() },
                enabled = !busy && username.isNotBlank() && password.isNotEmpty(),
                modifier = Modifier.fillMaxWidth(),
            ) { Text(if (busy) "Please wait…" else if (setupRequired) "Create and sign in" else if (signingUp) "Create account" else "Sign in") }
            if (!setupRequired) {
                TextButton(onClick = { signingUp = !signingUp; error = null }) {
                    Text(if (signingUp) "I already have an account" else "Create an account")
                }
            }
            Text(
                "Use HTTPS outside the local network. The emulator reaches your computer at http://10.0.2.2:3000.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
