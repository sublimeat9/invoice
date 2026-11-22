package app.invoicing.android.data

private val AMOUNT = Regex("""^(\d{1,9})(?:[.,](\d{1,2}))?$""")

/** "15", "15.5" or "15,50" to integer cents, without floating point. `null` if invalid. */
fun parseCents(text: String): Long? {
    val m = AMOUNT.matchEntire(text.trim()) ?: return null
    val cents = m.groupValues[2].padEnd(2, '0').ifEmpty { "0" }
    return m.groupValues[1].toLong() * 100 + cents.toLong()
}

fun formatMoney(cents: Long): String = "$%d.%02d".format(cents / 100, cents % 100)

fun centsToText(cents: Long): String = "%d.%02d".format(cents / 100, cents % 100)

data class Totals(val subtotal: Long, val vat: Long, val total: Long)

private fun Vat.rate() = when (this) {
    Vat.FIFTEEN -> 15
    Vat.FIVE -> 5
    else -> 0
}

/** Same calculation as the backend: VAT per line, rounded half-up to cents. */
fun totalsOf(lines: List<InvoiceLine>): Totals {
    var subtotal = 0L
    var vat = 0L
    for (l in lines) {
        val base = l.quantity * l.unitPriceCents - l.discountCents
        subtotal += base
        vat += (base * l.vat.rate() + 50) / 100
    }
    return Totals(subtotal, vat, subtotal + vat)
}
