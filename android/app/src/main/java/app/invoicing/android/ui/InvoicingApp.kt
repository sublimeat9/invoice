package app.invoicing.android.ui

import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material.icons.filled.ShoppingCart
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavType
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import androidx.navigation.navArgument
import app.invoicing.android.data.ApiClient
import app.invoicing.android.data.SecureSettings

private enum class Tab(val route: String, val label: String, val icon: ImageVector) {
    Documents("documents", "Documents", Icons.Default.Home),
    Products("products", "Products", Icons.Default.ShoppingCart),
    Customers("customers", "Customers", Icons.Default.Person),
    Settings("settings", "Settings", Icons.Default.Settings),
}

@Composable
fun InvoicingApp(api: ApiClient, settings: SecureSettings) {
    val nav = rememberNavController()
    val backStack by nav.currentBackStackEntryAsState()
    val current = backStack?.destination?.route

    Scaffold(
        bottomBar = {
            // Only the top-level screens show the bar; forms and details are full screen.
            if (Tab.entries.any { it.route == current }) {
                NavigationBar {
                    Tab.entries.forEach { tab ->
                        NavigationBarItem(
                            selected = current == tab.route,
                            onClick = {
                                nav.navigate(tab.route) {
                                    popUpTo(nav.graph.findStartDestination().id) { saveState = true }
                                    launchSingleTop = true
                                    restoreState = true
                                }
                            },
                            icon = { Icon(tab.icon, contentDescription = null) },
                            label = { Text(tab.label) },
                        )
                    }
                }
            }
        },
    ) { padding ->
        NavHost(nav, startDestination = Tab.Documents.route, modifier = Modifier.padding(padding)) {
            composable(Tab.Documents.route) {
                DocumentsScreen(
                    api,
                    onOpen = { nav.navigate("invoice/$it") },
                    onNew = { nav.navigate("new-invoice") },
                )
            }
            composable("invoice/{accessKey}", listOf(navArgument("accessKey") { type = NavType.StringType })) {
                InvoiceDetailScreen(api, it.arguments!!.getString("accessKey")!!, onBack = { nav.popBackStack() })
            }
            composable("new-invoice") {
                NewInvoiceScreen(
                    api,
                    onBack = { nav.popBackStack() },
                    onIssued = { key ->
                        nav.navigate("invoice/$key") { popUpTo("new-invoice") { inclusive = true } }
                    },
                )
            }
            composable(Tab.Products.route) {
                ProductsScreen(api, onOpen = { nav.navigate("product/$it") })
            }
            composable("product/{id}") {
                ProductFormScreen(api, it.arguments!!.getString("id")!!, onDone = { nav.popBackStack() })
            }
            composable(Tab.Customers.route) {
                CustomersScreen(api, onOpen = { nav.navigate("customer/$it") })
            }
            composable("customer/{id}") {
                CustomerFormScreen(api, it.arguments!!.getString("id")!!, onDone = { nav.popBackStack() })
            }
            composable(Tab.Settings.route) { SettingsScreen(api, settings) }
        }
    }
}
