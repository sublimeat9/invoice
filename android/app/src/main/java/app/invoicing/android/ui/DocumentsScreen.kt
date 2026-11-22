package app.invoicing.android.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.formatMoney
import kotlinx.coroutines.launch

private const val PAGE_SIZE = 20
private val STATUSES = listOf("" to "All", "AUTORIZADO" to "Authorized", "PENDIENTE" to "Pending", "RECHAZADO" to "Rejected")

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DocumentsScreen(api: ApiClient, onOpen: (String) -> Unit, onNew: () -> Unit) {
    var idNumber by rememberSaveable { mutableStateOf("") }
    var status by rememberSaveable { mutableStateOf("") }
    var page by rememberSaveable { mutableIntStateOf(1) }
    var notice by remember { mutableStateOf<String?>(null) }
    var retrying by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()

    val result = rememberRemote(Triple(idNumber, status, page)) { api.searchInvoices(idNumber, status, page, PAGE_SIZE) }
    val data = result.value
    val pages = data?.let { maxOf(1, (it.total + PAGE_SIZE - 1) / PAGE_SIZE) } ?: 1
    val hasPending = data?.items?.any { it.status == "PENDIENTE" } == true

    Scaffold(
        topBar = { TopAppBar(title = { Text("Documents") }) },
        floatingActionButton = {
            ExtendedFloatingActionButton(onClick = onNew, icon = { Icon(Icons.Default.Add, null) }, text = { Text("New invoice") })
        },
    ) { padding ->
        LazyColumn(
            Modifier.padding(padding).fillMaxSize(),
            contentPadding = PaddingValues(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            item {
                OutlinedTextField(
                    value = idNumber,
                    onValueChange = { idNumber = it; page = 1 },
                    label = { Text("ID number") },
                    singleLine = true,
                    modifier = Modifier.fillMaxWidth(),
                )
            }
            item {
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    STATUSES.forEach { (code, label) ->
                        FilterChip(selected = status == code, onClick = { status = code; page = 1 }, label = { Text(label) })
                    }
                }
            }
            result.error?.let { item { ErrorNotice(it) } }
            notice?.let { item { InfoNotice(it) } }
            if (hasPending) {
                item {
                    InfoNotice(if (retrying) "Retrying…" else "Some invoices are pending: the SRI did not answer.")
                    TextButton(
                        enabled = !retrying,
                        onClick = {
                            scope.launch {
                                retrying = true
                                notice = try {
                                    "Retried ${api.retryPending()} pending documents."
                                } catch (e: Exception) {
                                    e.userMessage()
                                }
                                retrying = false
                                result.reload()
                            }
                        },
                    ) { Text("Retry now") }
                }
            }
            if (result.loading && data == null) item { Loading() }
            data?.let { d ->
                if (d.items.isEmpty() && result.error == null) item { EmptyState("No results found.") }
                items(d.items, key = { it.accessKey }) { invoice ->
                    Card(
                        Modifier.fillMaxWidth().clickable { onOpen(invoice.accessKey) },
                        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
                    ) {
                        Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                                Text(invoice.number, style = MaterialTheme.typography.titleMedium)
                                StatusBadge(invoice.status)
                            }
                            Text(invoice.businessName ?: "—", maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                                Text(
                                    listOfNotNull(invoice.issueDate, invoice.idNumber).joinToString(" · "),
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                                Text(invoice.totalAmount?.let(::formatMoney) ?: "—", style = MaterialTheme.typography.titleSmall)
                            }
                        }
                    }
                }
                item {
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween, verticalAlignment = Alignment.CenterVertically) {
                        TextButton(enabled = page > 1, onClick = { page-- }) { Text("Previous") }
                        Text("${d.total} results · $page / $pages", style = MaterialTheme.typography.bodySmall)
                        TextButton(enabled = page < pages, onClick = { page++ }) { Text("Next") }
                    }
                }
            }
        }
    }
}
