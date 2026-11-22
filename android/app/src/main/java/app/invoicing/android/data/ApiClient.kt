package app.invoicing.android.data

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder

class ApiException(val status: Int, message: String) : Exception(message)

/**
 * Client of the backend HTTP API (`/invoices`, `/products`, `/customers`, `/config`).
 * Uses only the platform's HttpURLConnection and org.json, so it adds no dependencies.
 */
class ApiClient(
    private val connection: () -> Connection,
    /** Called when the server rejects the session token (expired, revoked or user disabled). */
    private val onUnauthorized: () -> Unit = {},
) {

    data class Connection(val baseUrl: String, val token: String)

    private suspend fun request(method: String, path: String, body: JSONObject? = null): String =
        withContext(Dispatchers.IO) {
            val conn = connection()
            if (conn.baseUrl.isBlank()) throw ApiException(0, "Enter the server URL")
            val http = try {
                URL(conn.baseUrl.trimEnd('/') + path).openConnection() as HttpURLConnection
            } catch (e: IOException) {
                throw ApiException(0, "Invalid server URL")
            }
            try {
                http.requestMethod = method
                http.connectTimeout = 10_000
                // Issuing waits for the SRI, which can take a while.
                http.readTimeout = 90_000
                if (conn.token.isNotBlank()) http.setRequestProperty("Authorization", "Bearer ${conn.token}")
                if (body != null) {
                    http.doOutput = true
                    http.setRequestProperty("Content-Type", "application/json")
                    http.outputStream.use { it.write(body.toString().toByteArray()) }
                }
                val status = http.responseCode
                val stream = if (status in 200..299) http.inputStream else http.errorStream
                val text = stream?.bufferedReader()?.use { it.readText() } ?: ""
                if (status == 401 && path != "/auth/login" && path != "/auth/setup" && path != "/auth/register") onUnauthorized()
                if (status !in 200..299) throw ApiException(status, errorMessage(status, text))
                text
            } catch (e: ApiException) {
                throw e
            } catch (e: IOException) {
                throw ApiException(0, "Could not reach the server")
            } finally {
                http.disconnect()
            }
        }

    private fun errorMessage(status: Int, body: String): String =
        runCatching { JSONObject(body).getString("error") }.getOrNull()
            ?: "Error $status"

    private fun query(vararg params: Pair<String, Any?>): String {
        val parts = params.filter { it.second != null && it.second.toString().isNotBlank() }
            .joinToString("&") { "${it.first}=${URLEncoder.encode(it.second.toString(), "UTF-8")}" }
        return if (parts.isEmpty()) "" else "?$parts"
    }

    // ---------- accounts ----------

    suspend fun setupRequired(): Boolean =
        JSONObject(request("GET", "/auth/status")).getBoolean("setup_required")

    suspend fun login(username: String, password: String): LoginResult = LoginResult.from(
        JSONObject(request("POST", "/auth/login", JSONObject().put("username", username).put("password", password))),
    )

    /** First run only: creates the administrator and signs in. */
    suspend fun createAdmin(username: String, password: String): LoginResult = LoginResult.from(
        JSONObject(request("POST", "/auth/setup", JSONObject().put("username", username).put("password", password))),
    )

    /** Public sign-up: the RUC is the username; the issuer data is fetched from the SRI. */
    suspend fun register(ruc: String, password: String): LoginResult = LoginResult.from(
        JSONObject(request("POST", "/auth/register", JSONObject().put("ruc", ruc).put("password", password))),
    )

    suspend fun logout() {
        request("POST", "/auth/logout")
    }

    suspend fun changePassword(current: String, new: String) {
        request("PUT", "/auth/password", JSONObject().put("current_password", current).put("new_password", new))
    }

    // ---------- invoices ----------

    suspend fun searchInvoices(idNumber: String, status: String, page: Int, limit: Int = 20): Page<InvoiceRecord> {
        val json = JSONObject(
            request("GET", "/invoices" + query("id_number" to idNumber, "status" to status, "page" to page, "limit" to limit)),
        )
        return Page(json.getInt("total"), json.getJSONArray("items").map(InvoiceRecord::from))
    }

    suspend fun getInvoice(accessKey: String): InvoiceRecord =
        InvoiceRecord.from(JSONObject(request("GET", "/invoices/${URLEncoder.encode(accessKey, "UTF-8")}")))

    suspend fun issueInvoice(invoice: NewInvoice): InvoiceRecord =
        InvoiceRecord.from(JSONObject(request("POST", "/invoices", invoice.toJson())))

    /** Returns how many pending invoices were retried. */
    suspend fun retryPending(): Int = JSONArray(request("POST", "/invoices/retry")).length()

    // ---------- settings ----------

    suspend fun config(): ServerConfig = ServerConfig.from(JSONObject(request("GET", "/config")))

    // ---------- catalogs ----------

    suspend fun products(q: String, page: Int = 1, limit: Int = 50): Page<Product> {
        val json = JSONObject(request("GET", "/products" + query("q" to q, "page" to page, "limit" to limit)))
        return Page(json.getInt("total"), json.getJSONArray("items").map(Product::from))
    }

    suspend fun saveProduct(id: Long?, product: Product): Product {
        val response = if (id == null) request("POST", "/products", product.toJson())
        else request("PUT", "/products/$id", product.toJson())
        return Product.from(JSONObject(response))
    }

    suspend fun deleteProduct(id: Long) {
        request("DELETE", "/products/$id")
    }

    suspend fun customers(q: String, page: Int = 1, limit: Int = 50): Page<Customer> {
        val json = JSONObject(request("GET", "/customers" + query("q" to q, "page" to page, "limit" to limit)))
        return Page(json.getInt("total"), json.getJSONArray("items").map(Customer::from))
    }

    suspend fun saveCustomer(id: Long?, customer: Customer): Customer {
        val response = if (id == null) request("POST", "/customers", customer.toJson())
        else request("PUT", "/customers/$id", customer.toJson())
        return Customer.from(JSONObject(response))
    }

    suspend fun deleteCustomer(id: Long) {
        request("DELETE", "/customers/$id")
    }
}

private fun <T> JSONArray.map(transform: (JSONObject) -> T): List<T> =
    (0 until length()).map { transform(getJSONObject(it)) }
