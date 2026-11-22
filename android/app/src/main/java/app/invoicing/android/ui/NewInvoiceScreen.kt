package app.invoicing.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.ui.unit.dp
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.Buyer
import app.invoicing.android.data.InvoiceLine
import app.invoicing.android.data.NewInvoice
import app.invoicing.android.data.Product
import app.invoicing.android.data.Vat
import app.invoicing.android.data.centsToText
import app.invoicing.android.data.formatMoney
import app.invoicing.android.data.parseCents
import app.invoicing.android.data.totalsOf
import kotlinx.coroutines.launch

/** Editable text state of one invoice line. */
private class LineForm {
    var code by mutableStateOf("")
    var description by mutableStateOf("")
    var quantity by mutableStateOf("1")
    var price by mutableStateOf("")
    var discount by mutableStateOf("")
    var vat by mutableStateOf(Vat.FIFTEEN)

    fun fill(p: Product) {
        code = p.mainCode
        description = p.name
        price = centsToText(p.priceCents)
        vat = p.vat
    }
}

private val ID_TYPES = listOf("05" to "Cédula", "04" to "RUC", "06" to "Passport", "08" to "Foreign ID")
private val PAYMENTS = listOf("01" to "Without financial system", "16" to "Debit card", "19" to "Credit card", "17" to "Electronic money")

/** Validates a line; returns the API line or an error message. */
private fun LineForm.build(n: Int): Pair<InvoiceLine?, String?> {
    val qty = quantity.trim().toIntOrNull()
    val unit = parseCents(price)
    val off = parseCents(discount.ifBlank { "0" })
    return when {
        description.isBlank() -> null to "Line $n: description is missing"
        qty == null || qty < 1 -> null to "Line $n: quantity must be an integer greater than 0"
        unit == null || unit <= 0 -> null to "Line $n: invalid price (up to 2 decimals)"
        off == null || off > qty * unit -> null to "Line $n: invalid discount"
        else -> InvoiceLine(code.trim().ifEmpty { "S/C" }, description.trim(), qty, unit, off, vat) to null
    }
}

@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun NewInvoiceScreen(api: ApiClient, onBack: () -> Unit, onIssued: (String) -> Unit) {
    val scope = rememberCoroutineScope()
    val catalog = rememberRemote(Unit) { api.products("", 1, 200).items }
    val lines = remember { mutableStateListOf(LineForm()) }

    var finalConsumer by remember { mutableStateOf(true) }
    var idType by remember { mutableStateOf("05") }
    var idNumber by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }
    var address by remember { mutableStateOf("") }
    var email by remember { mutableStateOf("") }
    var payment by remember { mutableStateOf("01") }
    var error by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }

    val totals = totalsOf(
        lines.mapNotNull { l ->
            val q = l.quantity.toIntOrNull() ?: return@mapNotNull null
            InvoiceLine("", "", q, parseCents(l.price) ?: 0, parseCents(l.discount.ifBlank { "0" }) ?: 0, l.vat)
        },
    )

    fun submit() {
        error = null
        val built = lines.mapIndexed { i, l -> l.build(i + 1) }
        built.firstNotNullOfOrNull { it.second }?.let { error = it; return }
        val buyer = if (finalConsumer) null else {
            val id = idNumber.trim()
            if (name.isBlank() || id.isEmpty()) { error = "Fill in the buyer name and ID number"; return }
            if (idType == "05" && !Regex("""\d{10}""").matches(id)) { error = "A cédula must have 10 digits"; return }
            if (idType == "04" && !Regex("""\d{13}""").matches(id)) { error = "A RUC must have 13 digits"; return }
            Buyer(idType, name.trim(), id, address.trim().ifEmpty { null })
        }
        if (email.isNotBlank() && !Regex("""^[^@\s]+@[^@\s]+\.[^@\s]+$""").matches(email.trim())) {
            error = "Invalid email address"; return
        }
        val invoice = NewInvoice(
            buyer,
            built.mapNotNull { it.first },
            payment,
            if (email.isBlank()) emptyList() else listOf("Email" to email.trim()),
        )
        scope.launch {
            busy = true
            try {
                onIssued(api.issueInvoice(invoice).accessKey)
            } catch (e: Exception) {
                error = e.userMessage()
            }
            busy = false
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text("New invoice") },
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
            )
        },
    ) { padding ->
        Column(
            Modifier.padding(padding).padding(16.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            error?.let { ErrorNotice(it) }

            Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Buyer", style = MaterialTheme.typography.titleMedium)
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Checkbox(finalConsumer, { finalConsumer = it })
                    Text("Final consumer")
                }
                if (!finalConsumer) {
                    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        ID_TYPES.forEach { (code, label) ->
                            FilterChip(selected = idType == code, onClick = { idType = code }, label = { Text(label) })
                        }
                    }
                    OutlinedTextField(idNumber, { idNumber = it }, label = { Text("ID number") }, singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number), modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(name, { name = it }, label = { Text("Name / business name") }, modifier = Modifier.fillMaxWidth())
                    OutlinedTextField(address, { address = it }, label = { Text("Address (optional)") }, modifier = Modifier.fillMaxWidth())
                }
                OutlinedTextField(email, { email = it }, label = { Text("Email (optional)") }, singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email), modifier = Modifier.fillMaxWidth())
            } }

            Text("Lines", style = MaterialTheme.typography.titleMedium)
            lines.forEachIndexed { i, line ->
                LineCard(line, catalog.value.orEmpty(), canRemove = lines.size > 1, onRemove = { lines.removeAt(i) })
            }
            OutlinedButton(onClick = { lines.add(LineForm()) }) { Text("+ Add line") }

            Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text("Payment", style = MaterialTheme.typography.titleMedium)
                PAYMENTS.forEach { (code, label) ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        androidx.compose.material3.RadioButton(payment == code, { payment = code })
                        Text(label)
                    }
                }
            } }

            Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                SummaryRow("Subtotal", formatMoney(totals.subtotal))
                SummaryRow("VAT", formatMoney(totals.vat))
                SummaryRow("Total", formatMoney(totals.total), bold = true)
            } }

            Button(onClick = ::submit, enabled = !busy, modifier = Modifier.fillMaxWidth()) {
                Text(if (busy) "Sending to the SRI…" else "Issue invoice")
            }
        }
    }
}

@Composable
private fun SummaryRow(label: String, value: String, bold: Boolean = false) {
    val style = if (bold) MaterialTheme.typography.titleMedium else MaterialTheme.typography.bodyLarge
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
        Text(label, style = style)
        Text(value, style = style)
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun LineCard(line: LineForm, catalog: List<Product>, canRemove: Boolean, onRemove: () -> Unit) {
    var menu by remember { mutableStateOf(false) }
    Card(Modifier.fillMaxWidth()) { Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        if (catalog.isNotEmpty()) {
            Column {
                TextButton(onClick = { menu = true }) { Text("Pick from catalog") }
                DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
                    catalog.forEach { p ->
                        DropdownMenuItem(
                            text = { Text("${p.mainCode} · ${p.name}") },
                            onClick = { line.fill(p); menu = false },
                        )
                    }
                }
            }
        }
        OutlinedTextField(line.description, { line.description = it }, label = { Text("Description") }, modifier = Modifier.fillMaxWidth())
        TwoColumns(
            { OutlinedTextField(line.code, { line.code = it }, label = { Text("Code") }, placeholder = { Text("S/C") }, singleLine = true) },
            { OutlinedTextField(line.quantity, { line.quantity = it }, label = { Text("Quantity") }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number)) },
        )
        TwoColumns(
            { OutlinedTextField(line.price, { line.price = it }, label = { Text("Unit price") }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal)) },
            { OutlinedTextField(line.discount, { line.discount = it }, label = { Text("Discount") }, singleLine = true,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Decimal)) },
        )
        FlowRow(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Vat.entries.forEach { v -> FilterChip(selected = line.vat == v, onClick = { line.vat = v }, label = { Text(v.label) }) }
        }
        if (canRemove) TextButton(onClick = onRemove) { Text("Remove line", color = MaterialTheme.colorScheme.error) }
    } }
}
