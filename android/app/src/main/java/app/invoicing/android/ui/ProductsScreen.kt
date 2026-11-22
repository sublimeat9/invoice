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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.formatMoney

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ProductsScreen(api: ApiClient, onOpen: (String) -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    val products = rememberRemote(query) { api.products(query) }

    Scaffold(
        topBar = { TopAppBar(title = { Text("Products and services") }) },
        floatingActionButton = {
            ExtendedFloatingActionButton(onClick = { onOpen("new") }, icon = { Icon(Icons.Default.Add, null) }, text = { Text("New product") })
        },
    ) { padding ->
        LazyColumn(Modifier.padding(padding).fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            item {
                OutlinedTextField(query, { query = it }, label = { Text("Search by code or name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            }
            products.error?.let { item { ErrorNotice(it) } }
            if (products.loading && products.value == null) item { Loading() }
            products.value?.let { page ->
                if (page.items.isEmpty() && products.error == null) item { EmptyState("No products yet.") }
                items(page.items, key = { it.id }) { p ->
                    Card(Modifier.fillMaxWidth().clickable { onOpen(p.id.toString()) }) {
                        Row(Modifier.padding(14.dp).fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                            Column(Modifier.weight(1f)) {
                                Text(p.name, style = MaterialTheme.typography.titleMedium)
                                Text("${p.mainCode} · VAT ${p.vat.label}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                            }
                            Text(formatMoney(p.priceCents), style = MaterialTheme.typography.titleSmall)
                        }
                    }
                }
            }
        }
    }
}
