package org.vpnflow.vpn

import android.app.Activity
import android.content.Intent
import android.net.VpnService
import android.provider.Settings
import androidx.activity.result.ActivityResult
import app.tauri.annotation.ActivityCallback
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import com.wireguard.android.backend.GoBackend
import com.wireguard.android.backend.Tunnel
import com.wireguard.config.Config
import java.io.ByteArrayInputStream
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch

@InvokeArg
class ConnectArgs {
    lateinit var config: String
}

/**
 * Runs the WireGuard tunnel on Android's VpnService. The Rust side stores the
 * profiles and sends the finished wg-quick text here; nothing is kept on this
 * side except the running tunnel.
 */
@TauriPlugin
class VpnPlugin(private val activity: Activity) : Plugin(activity) {
    private val scope = CoroutineScope(Dispatchers.IO)
    private var backend: GoBackend? = null

    private val tunnel = object : Tunnel {
        override fun getName() = "vpnflow"
        override fun onStateChange(newState: Tunnel.State) {}
    }

    private fun backend(): GoBackend =
        backend ?: GoBackend(activity.applicationContext).also { backend = it }

    /** Asks Android for permission to create a VPN (one system dialog, once). */
    @Command
    fun prepare(invoke: Invoke) {
        val intent = VpnService.prepare(activity)
        if (intent == null) {
            invoke.resolve(JSObject().put("granted", true))
        } else {
            startActivityForResult(invoke, intent, "prepareResult")
        }
    }

    @ActivityCallback
    fun prepareResult(invoke: Invoke, result: ActivityResult) {
        invoke.resolve(JSObject().put("granted", result.resultCode == Activity.RESULT_OK))
    }

    @Command
    fun connect(invoke: Invoke) {
        val args = invoke.parseArgs(ConnectArgs::class.java)
        scope.launch {
            try {
                val config = Config.parse(ByteArrayInputStream(args.config.toByteArray()))
                backend().setState(tunnel, Tunnel.State.UP, config)
                invoke.resolve()
            } catch (e: Exception) {
                invoke.reject(e.message ?: e.toString())
            }
        }
    }

    @Command
    fun disconnect(invoke: Invoke) {
        scope.launch {
            try {
                backend?.setState(tunnel, Tunnel.State.DOWN, null)
                invoke.resolve()
            } catch (e: Exception) {
                invoke.reject(e.message ?: e.toString())
            }
        }
    }

    @Command
    fun status(invoke: Invoke) {
        scope.launch {
            try {
                val b = backend
                val out = JSObject()
                val up = b != null && b.getState(tunnel) == Tunnel.State.UP
                out.put("up", up)
                if (b != null && up) {
                    val stats = b.getStatistics(tunnel)
                    var handshake = 0L
                    for (key in stats.peers()) {
                        val peer = stats.peer(key)
                        if (peer != null && peer.latestHandshakeEpochMillis() > handshake) {
                            handshake = peer.latestHandshakeEpochMillis()
                        }
                    }
                    out.put("rx", stats.totalRx())
                    out.put("tx", stats.totalTx())
                    out.put("lastHandshakeMs", handshake)
                }
                invoke.resolve(out)
            } catch (e: Exception) {
                invoke.reject(e.message ?: e.toString())
            }
        }
    }

    /** Opens Android's VPN settings, where "Block connections without VPN" lives. */
    @Command
    fun openVpnSettings(invoke: Invoke) {
        val intent = Intent(Settings.ACTION_VPN_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        activity.startActivity(intent)
        invoke.resolve()
    }
}
