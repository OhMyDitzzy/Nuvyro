package com.ditzzy.nuvyro.core.transfer.receiver

import com.ditzzy.nuvyro.core.transfer.NativeCommandCodec
import com.ditzzy.nuvyro.core.transfer.endpoint.SecureSessionId
import java.net.Inet6Address

internal object ReceiverCommandCodec {
    fun encode(generation: Long, sessionId: String, routeGeneration: Long,
        routes: ReceiverRouteSnapshot, update: Boolean): ByteArray {
        require(generation > 0 && routeGeneration > 0 && SecureSessionId.parse(sessionId) != null)
        // Numeric platform values and hex bytes require no JSON escaping; peer text never enters this format.
        val locals = routes.localAddresses().joinToString(",") { local ->
            val scope = (local.address as? Inet6Address)?.scopeId ?: 0
            """{"address":"${hex(local.address.address)}","scope_id":"$scope","prefix_length":${local.prefixLength}}"""
        }
        val clients = routes.clientAddresses().joinToString(",") { "\"${hex(it.address)}\"" }
        val interfaces = routes.clientInterfaces().joinToString(",")
        val network = java.lang.Long.toUnsignedString(routes.networkHandle ?: 0L)
        val data = """{"route_generation":"$routeGeneration","network_handle":"$network","interface_index":${routes.interfaceIndex},"locals":[$locals],"client_addresses":[$clients],"client_interfaces":[$interfaces]}"""
        return NativeCommandCodec.encode(if (update) "updateReceiverRoutes" else "attachReceiver", generation, sessionId, data)
    }

    private fun hex(bytes: ByteArray): String = bytes.joinToString("") { "%02x".format(it.toInt() and 0xff) }
}
