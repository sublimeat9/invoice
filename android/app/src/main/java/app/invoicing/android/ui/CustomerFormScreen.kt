package app.invoicing.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.Customer
import kotlinx.coroutines.launch

private val ID_TYPES = listOf("05" to "Cédula", "04" to "RUC", "06" to "Passport", "08" to "Foreign ID")

/** Creates (`id == "new"`) or edits a customer. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun CustomerFormScreen(api: ApiClient, id: String, onDone: () -> Unit) {
    val customerId = id.toLongOrNull()
    val scope = rememberCoroutineScope()
    var idType by remember { mutableStateOf("05") }
    var idNumber by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }
    var address by remember { mutableStateOf("") }
    var phone by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }

    LaunchedEffect(customerId) {
        if (customerId == null) return@LaunchedEffect
        try {
            val c = api.customers("", 1, 1000).items.first { it.id == customerId }
            idType = c.idType; idNumber = c.idNumber; name = c.businessName
            address = c.address.orEmpty(); phone = c.phone.orEmpty(); email = c.email.orEmpty()
        } catch (e: Exception) {
            error = if (e is NoSuchElementException) "Customer not found" else e.userMessage()
        }
    }

    fun save() {
        scope.launch {
            busy = true; error = null
            try {
                api.saveCustomer(
                    customerId,
                    Customer(0, idType, idNumber.trim(), name.trim(), address.trim().ifEmpty { null }, phone.trim().ifEmpty { null }, email.trim().ifEmpty { null }),
                )
                onDone()
            } catch (e: Exception) { error = e.userMessage() }
            busy = false
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (customerId == null) "New customer" else "Edit customer") },
                navigationIcon = { IconButton(onClick = onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).padding(16.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            error?.let { ErrorNotice(it) }
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                ID_TYPES.forEach { (code, label) -> FilterChip(selected = idType == code, onClick = { idType = code }, label = { Text(label) }) }
            }
            OutlinedTextField(idNumber, { idNumber = it }, label = { Text("ID number") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(name, { name = it }, label = { Text("Name / business name") }, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(address, { address = it }, label = { Text("Address (optional)") }, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(phone, { phone = it }, label = { Text("Phone (optional)") }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Phone), modifier = Modifier.fillMaxWidth())
            OutlinedTextField(email, { email = it }, label = { Text("Email (optional)") }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email), modifier = Modifier.fillMaxWidth())
            Button(onClick = ::save, enabled = !busy, modifier = Modifier.fillMaxWidth()) { Text(if (busy) "Saving…" else "Save") }
            if (customerId != null) {
                OutlinedButton(onClick = { confirmDelete = true }, modifier = Modifier.fillMaxWidth()) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }

    if (confirmDelete && customerId != null) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete \"$name\"?") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    scope.launch {
                        try { api.deleteCustomer(customerId); onDone() } catch (e: Exception) { error = e.userMessage() }
                    }
                }) { Text("Delete") }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
}
