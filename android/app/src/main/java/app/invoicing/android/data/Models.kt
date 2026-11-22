package app.invoicing.android.data

import org.json.JSONArray
import org.json.JSONObject

/** Reads a nullable string: JSON `null` and missing keys both become Kotlin `null`. */
private fun JSONObject.optNullableString(key: String): String? =
    if (isNull(key)) null else getString(key)

private fun JSONObject.optNullableLong(key: String): Long? = if (isNull(key)) null else getLong(key)

data class Page<T>(val total: Int, val items: List<T>)

data class InvoiceRecord(
    val accessKey: String,
    val number: String,
    val issueDate: String?,
    val idNumber: String?,
    val businessName: String?,
    /** In cents. */
    val totalAmount: Long?,
    val status: String,
    val authorizationNumber: String?,
    val authorizationDate: String?,
    val messages: String?,
    val signedXml: String?,
) {
    companion object {
        fun from(j: JSONObject) = InvoiceRecord(
            accessKey = j.getString("access_key"),
            number = j.getString("number"),
            issueDate = j.optNullableString("issue_date"),
            idNumber = j.optNullableString("id_number"),
            businessName = j.optNullableString("business_name"),
            totalAmount = j.optNullableLong("total_amount"),
            status = j.getString("status"),
            authorizationNumber = j.optNullableString("authorization_number"),
            authorizationDate = j.optNullableString("authorization_date"),
            messages = j.optNullableString("messages"),
            signedXml = j.optNullableString("signed_xml"),
        )
    }
}

enum class Vat(val code: String, val label: String) {
    FIFTEEN("15", "15 %"),
    FIVE("5", "5 %"),
    ZERO("0", "0 %"),
    NOT_SUBJECT("not_subject", "Not subject"),
    EXEMPT("exempt", "Exempt");

    companion object {
        fun fromCode(code: String) = entries.firstOrNull { it.code == code } ?: FIFTEEN
    }
}

data class Product(
    val id: Long,
    val mainCode: String,
    val auxCode: String?,
    val name: String,
    val priceCents: Long,
    val vat: Vat,
) {
    fun toJson() = JSONObject()
        .put("main_code", mainCode)
        .put("aux_code", auxCode ?: JSONObject.NULL)
        .put("name", name)
        .put("price_cents", priceCents)
        .put("vat", vat.code)

    companion object {
        fun from(j: JSONObject) = Product(
            id = j.getLong("id"),
            mainCode = j.getString("main_code"),
            auxCode = j.optNullableString("aux_code"),
            name = j.getString("name"),
            priceCents = j.getLong("price_cents"),
            vat = Vat.fromCode(j.getString("vat")),
        )
    }
}

data class Customer(
    val id: Long,
    val idType: String,
    val idNumber: String,
    val businessName: String,
    val address: String?,
    val phone: String?,
    val email: String?,
) {
    fun toJson() = JSONObject()
        .put("id_type", idType)
        .put("id_number", idNumber)
        .put("business_name", businessName)
        .put("address", address ?: JSONObject.NULL)
        .put("phone", phone ?: JSONObject.NULL)
        .put("email", email ?: JSONObject.NULL)

    companion object {
        fun from(j: JSONObject) = Customer(
            id = j.getLong("id"),
            idType = j.getString("id_type"),
            idNumber = j.getString("id_number"),
            businessName = j.getString("business_name"),
            address = j.optNullableString("address"),
            phone = j.optNullableString("phone"),
            email = j.optNullableString("email"),
        )
    }
}

data class ServerConfig(
    val ready: Boolean,
    val environment: String,
    val issuerName: String?,
    val certificateLoaded: Boolean,
    val certificateValid: Boolean?,
    val certificateSubject: String?,
) {
    companion object {
        fun from(j: JSONObject): ServerConfig {
            val certificate = j.getJSONObject("certificate")
            val issuer = if (j.isNull("issuer")) null else j.getJSONObject("issuer")
            return ServerConfig(
                ready = j.getBoolean("ready"),
                environment = j.getString("environment"),
                issuerName = issuer?.getString("business_name"),
                certificateLoaded = certificate.getBoolean("loaded"),
                certificateValid = if (certificate.has("valid")) certificate.getBoolean("valid") else null,
                certificateSubject = certificate.optNullableString("subject"),
            )
        }
    }
}

data class InvoiceLine(
    val mainCode: String,
    val description: String,
    val quantity: Int,
    val unitPriceCents: Long,
    val discountCents: Long,
    val vat: Vat,
)

data class Buyer(val idType: String, val businessName: String, val idNumber: String, val address: String?)

data class NewInvoice(
    /** `null` means final consumer. */
    val buyer: Buyer?,
    val lines: List<InvoiceLine>,
    val paymentMethod: String,
    val additionalInfo: List<Pair<String, String>>,
) {
    fun toJson(): JSONObject {
        val json = JSONObject()
        buyer?.let {
            json.put(
                "buyer",
                JSONObject()
                    .put("id_type", it.idType)
                    .put("business_name", it.businessName)
                    .put("id_number", it.idNumber)
                    .put("address", it.address ?: JSONObject.NULL),
            )
        }
        json.put(
            "lines",
            JSONArray(
                lines.map {
                    JSONObject()
                        .put("main_code", it.mainCode)
                        .put("description", it.description)
                        .put("quantity", it.quantity)
                        .put("unit_price_cents", it.unitPriceCents)
                        .put("discount_cents", it.discountCents)
                        .put("vat", it.vat.code)
                },
            ),
        )
        json.put("payment_method", paymentMethod)
        json.put("additional_info", JSONArray(additionalInfo.map { JSONArray(listOf(it.first, it.second)) }))
        return json
    }
}
