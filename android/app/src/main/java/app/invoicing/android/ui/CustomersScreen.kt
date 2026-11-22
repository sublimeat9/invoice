package app.invoicing.android.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CustomersScreen(api: ApiClient, onOpen: (String) -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    val customers = rememberRemote(query) { api.customers(query) }

    Scaffold(
        topBar = { TopAppBar(title = { Text("Customers") }) },
        floatingActionButton = {
            ExtendedFloatingActionButton(onClick = { onOpen("new") }, icon = { Icon(Icons.Default.Add, null) }, text = { Text("New customer") })
        },
    ) { padding ->
        LazyColumn(Modifier.padding(padding).fillMaxSize(), contentPadding = PaddingValues(16.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            item {
                OutlinedTextField(query, { query = it }, label = { Text("Search by ID number or name") }, singleLine = true, modifier = Modifier.fillMaxWidth())
            }
            customers.error?.let { item { ErrorNotice(it) } }
            if (customers.loading && customers.value == null) item { Loading() }
            customers.value?.let { page ->
                if (page.items.isEmpty() && customers.error == null) item { EmptyState("No customers yet.") }
                items(page.items, key = { it.id }) { c ->
                    Card(Modifier.fillMaxWidth().clickable { onOpen(c.id.toString()) }) {
                        Column(Modifier.padding(14.dp)) {
                            Text(c.businessName, style = MaterialTheme.typography.titleMedium)
                            Text(
                                listOfNotNull(c.idNumber, c.email).joinToString(" · "),
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                }
            }
        }
    }
}
