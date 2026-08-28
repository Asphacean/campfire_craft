package pub.campfire.auth.network;

import net.minecraftforge.fml.common.network.NetworkRegistry;
import net.minecraftforge.fml.common.network.simpleimpl.SimpleNetworkWrapper;
import net.minecraftforge.fml.relauncher.Side;

/**
 * Channel name "campfireauth" is 12 characters — comfortably under Forge
 * 1.12.2's hard-coded 20-character channel-name limit (RESEARCH.md
 * Pitfall 4, this plan's ServerAuthHandler read_first).
 */
public final class NetworkHandler {
    public static final SimpleNetworkWrapper INSTANCE =
            NetworkRegistry.INSTANCE.newSimpleChannel("campfireauth");

    private NetworkHandler() {
    }

    public static void init() {
        INSTANCE.registerMessage(AuthRequestMessage.Handler.class, AuthRequestMessage.class, 0, Side.CLIENT);
        INSTANCE.registerMessage(AuthResponseMessage.Handler.class, AuthResponseMessage.class, 1, Side.SERVER);
    }
}
