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
import app.invoicing.android.data.Product
import app.invoicing.android.data.Vat
import app.invoicing.android.data.centsToText
import app.invoicing.android.data.parseCents
import kotlinx.coroutines.launch

/** Creates (`id == "new"`) or edits a catalog product. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun ProductFormScreen(api: ApiClient, id: String, onDone: () -> Unit) {
    val productId = id.toLongOrNull()
    val scope = rememberCoroutineScope()
    var mainCode by remember { mutableStateOf("") }
    var auxCode by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }
    var price by remember { mutableStateOf("") }
    var vat by remember { mutableStateOf(Vat.FIFTEEN) }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    var confirmDelete by remember { mutableStateOf(false) }

    LaunchedEffect(productId) {
        if (productId == null) return@LaunchedEffect
        try {
            val p = api.products("", 1, 1000).items.first { it.id == productId }
            mainCode = p.mainCode; auxCode = p.auxCode.orEmpty(); name = p.name; price = centsToText(p.priceCents); vat = p.vat
        } catch (e: Exception) {
            error = if (e is NoSuchElementException) "Product not found" else e.userMessage()
        }
    }

    fun save() {
        val cents = parseCents(price)
        if (cents == null || cents <= 0) { error = "Invalid price (use up to 2 decimals)"; return }
        scope.launch {
            busy = true; error = null
            try {
                api.saveProduct(productId, Product(0, mainCode.trim(), auxCode.trim().ifEmpty { null }, name.trim(), cents, vat))
                onDone()
            } catch (e: Exception) { error = e.userMessage() }
            busy = false
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(if (productId == null) "New product" else "Edit product") },
                navigationIcon = { IconButton(onClick = onDone) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).padding(16.dp).verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            error?.let { ErrorNotice(it) }
            OutlinedTextField(mainCode, { mainCode = it }, label = { Text("Main code") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(auxCode, { auxCode = it }, label = { Text("Aux. code (optional)") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(name, { name = it }, label = { Text("Name / description") }, modifier = Modifier.fillMaxWidth())
            OutlinedTextField(price, { price = it }, label = { Text("Unit price") }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal), modifier = Modifier.fillMaxWidth())
            FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Vat.entries.forEach { v -> FilterChip(selected = vat == v, onClick = { vat = v }, label = { Text(v.label) }) }
            }
            Button(onClick = ::save, enabled = !busy, modifier = Modifier.fillMaxWidth()) { Text(if (busy) "Saving…" else "Save") }
            if (productId != null) {
                OutlinedButton(onClick = { confirmDelete = true }, modifier = Modifier.fillMaxWidth()) {
                    Text("Delete", color = MaterialTheme.colorScheme.error)
                }
            }
        }
    }

    if (confirmDelete && productId != null) {
        AlertDialog(
            onDismissRequest = { confirmDelete = false },
            title = { Text("Delete \"$name\"?") },
            confirmButton = {
                TextButton(onClick = {
                    confirmDelete = false
                    scope.launch {
                        try { api.deleteProduct(productId); onDone() } catch (e: Exception) { error = e.userMessage() }
                    }
                }) { Text("Delete") }
            },
            dismissButton = { TextButton(onClick = { confirmDelete = false }) { Text("Cancel") } },
        )
    }
}
