package app.invoicing.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import app.invoicing.android.data.ApiException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch

fun Throwable.userMessage(): String = (this as? ApiException)?.message ?: "Unexpected error"

/** A value loaded from the server, with its loading and error state. */
class Remote<T>(private val scope: CoroutineScope, private val fetch: suspend () -> T) {
    var value by mutableStateOf<T?>(null)
        private set
    var error by mutableStateOf<String?>(null)
        private set
    var loading by mutableStateOf(false)
        private set

    fun reload() {
        scope.launch {
            loading = true
            error = null
            try {
                value = fetch()
            } catch (e: Exception) {
                error = e.userMessage()
            }
            loading = false
        }
    }
}

/** Loads on first composition and again whenever [key] changes. */
@Composable
fun <T> rememberRemote(key: Any?, fetch: suspend () -> T): Remote<T> {
    val scope = rememberCoroutineScope()
    val remote = remember(key) { Remote(scope, fetch) }
    LaunchedEffect(remote) { remote.reload() }
    return remote
}

@Composable
fun ErrorNotice(message: String, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(8.dp),
        color = MaterialTheme.colorScheme.errorContainer,
    ) {
        Text(
            message,
            modifier = Modifier.padding(12.dp),
            color = MaterialTheme.colorScheme.onErrorContainer,
            style = MaterialTheme.typography.bodyMedium,
        )
    }
}

@Composable
fun InfoNotice(message: String, modifier: Modifier = Modifier) {
    Surface(
        modifier = modifier.fillMaxWidth(),
        shape = RoundedCornerShape(8.dp),
        color = MaterialTheme.colorScheme.secondaryContainer,
    ) {
        Text(message, modifier = Modifier.padding(12.dp), style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
fun Loading() {
    Box(Modifier.fillMaxWidth().padding(24.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
}

@Composable
fun EmptyState(text: String) {
    Box(Modifier.fillMaxWidth().padding(32.dp), contentAlignment = Alignment.Center) {
        Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** SRI status as a colored pill. */
@Composable
fun StatusBadge(status: String) {
    val (background, foreground) = when (status) {
        "AUTORIZADO" -> Color(0xFFDCFCE7) to Color(0xFF166534)
        "PENDIENTE" -> Color(0xFFFEF3C7) to Color(0xFF92400E)
        else -> Color(0xFFFEE2E2) to Color(0xFF991B1B)
    }
    Surface(shape = RoundedCornerShape(999.dp), color = background) {
        Text(
            status,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 2.dp),
            color = foreground,
            style = MaterialTheme.typography.labelSmall,
        )
    }
}

@Composable
fun LabeledValue(label: String, value: String?) {
    Column(Modifier.padding(vertical = 4.dp)) {
        Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(value ?: "—", style = MaterialTheme.typography.bodyLarge)
    }
}

@Composable
fun SpacedColumn(modifier: Modifier = Modifier, content: @Composable () -> Unit) {
    Column(modifier, verticalArrangement = Arrangement.spacedBy(12.dp)) { content() }
}

@Composable
fun TwoColumns(left: @Composable () -> Unit, right: @Composable () -> Unit) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Box(Modifier.weight(1f)) { left() }
        Box(Modifier.weight(1f)) { right() }
    }
}
