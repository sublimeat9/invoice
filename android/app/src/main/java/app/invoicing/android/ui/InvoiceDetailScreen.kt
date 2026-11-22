package app.invoicing.android.ui

import android.content.Intent
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.formatMoney

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun InvoiceDetailScreen(api: ApiClient, accessKey: String, onBack: () -> Unit) {
    val invoice = rememberRemote(accessKey) { api.getInvoice(accessKey) }
    val context = LocalContext.current

    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(invoice.value?.let { "Invoice ${it.number}" } ?: "Invoice") },
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") } },
            )
        },
    ) { padding ->
        Column(Modifier.padding(padding).padding(16.dp).verticalScroll(rememberScrollState())) {
            invoice.error?.let { ErrorNotice(it) }
            if (invoice.loading && invoice.value == null) Loading()
            invoice.value?.let { r ->
                Row { StatusBadge(r.status) }
                Card(Modifier.padding(vertical = 12.dp)) {
                    Column(Modifier.padding(16.dp)) {
                        LabeledValue("Issue date", r.issueDate)
                        LabeledValue("Buyer", listOfNotNull(r.businessName, r.idNumber).joinToString(" · ").ifEmpty { null })
                        LabeledValue("Total", r.totalAmount?.let(::formatMoney))
                        LabeledValue("Access key", r.accessKey)
                        LabeledValue("Authorization number", r.authorizationNumber)
                        LabeledValue("Authorization date", r.authorizationDate)
                    }
                }
                r.messages?.let {
                    Card(Modifier.padding(bottom = 12.dp)) {
                        Column(Modifier.padding(16.dp)) {
                            Text("SRI messages", style = MaterialTheme.typography.titleSmall)
                            Text(it, style = MaterialTheme.typography.bodySmall)
                        }
                    }
                }
                r.signedXml?.let { xml ->
                    Button(onClick = {
                        val send = Intent(Intent.ACTION_SEND).apply {
                            type = "text/xml"
                            putExtra(Intent.EXTRA_SUBJECT, "${r.accessKey}.xml")
                            putExtra(Intent.EXTRA_TEXT, xml)
                        }
                        context.startActivity(Intent.createChooser(send, "Share signed XML"))
                    }) { Text("Share signed XML") }
                }
            }
        }
    }
}
