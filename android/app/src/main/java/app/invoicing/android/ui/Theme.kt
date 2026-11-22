package app.invoicing.android.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

private val Accent = Color(0xFF2563EB)

private val Light = lightColorScheme(
    primary = Accent,
    background = Color(0xFFF7F8FA),
    surface = Color(0xFFFFFFFF),
    surfaceVariant = Color(0xFFEEF1F6),
    primaryContainer = Color(0xFFDBE4FF),
    onPrimaryContainer = Color(0xFF1E3A8A),
    secondaryContainer = Color(0xFFDBE4FF),
    onSecondaryContainer = Color(0xFF1E3A8A),
    surfaceContainer = Color(0xFFFFFFFF),
    surfaceContainerHigh = Color(0xFFF1F3F6),
    surfaceContainerHighest = Color(0xFFF1F3F6),
)

private val Dark = darkColorScheme(
    primary = Color(0xFF8AA2FF),
    background = Color(0xFF0F1217),
    surface = Color(0xFF161B22),
    surfaceVariant = Color(0xFF1F2630),
    primaryContainer = Color(0xFF26345F),
    onPrimaryContainer = Color(0xFFDBE4FF),
    secondaryContainer = Color(0xFF26345F),
    onSecondaryContainer = Color(0xFFDBE4FF),
    surfaceContainer = Color(0xFF161B22),
    surfaceContainerHigh = Color(0xFF1F2630),
    surfaceContainerHighest = Color(0xFF1F2630),
)

@Composable
fun InvoicingTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = if (isSystemInDarkTheme()) Dark else Light, content = content)
}
