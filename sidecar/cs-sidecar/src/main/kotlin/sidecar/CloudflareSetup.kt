package sidecar

import com.lagradost.cloudstream3.app
import com.lagradost.cloudstream3.insecureApp
import com.lagradost.nicehttp.Requests
import com.lagradost.runtime.loader.stubs.CloudflareSolver
import java.io.File

/**
 * Turns on Cloudflare verification for every HTTP client plugins reach through `app`: the folder the
 * app watches for verification requests is `<data folder>/cloudflare`, and the challenge interceptor
 * is added to the shared clients. Without a data folder (tests) it does nothing.
 */
object CloudflareSetup {
    fun install() {
        val dataDir = System.getProperty("cloudstream.data.dir")
        if (dataDir.isNullOrBlank()) {
            CloudflareSolver.log("WARN", "no data folder: Cloudflare verification is off")
            return
        }
        CloudflareSolver.exchangeDir = File(dataDir, "cloudflare")
        addTo(app)
        runCatching { addTo(insecureApp) }
        CloudflareSolver.log("INFO", "verification is on; exchange folder ${CloudflareSolver.exchangeDir}; the app answers within ${CloudflareSolver.solveTimeoutMs / 1000}s at most")
    }

    private fun addTo(requests: Requests) {
        val client = requests.baseClient
        if (client.interceptors.contains(CloudflareSolver.interceptor)) return
        requests.baseClient = client.newBuilder().addInterceptor(CloudflareSolver.interceptor).build()
    }
}
