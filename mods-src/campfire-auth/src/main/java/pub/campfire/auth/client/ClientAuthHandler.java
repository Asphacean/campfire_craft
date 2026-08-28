package pub.campfire.auth.client;

import net.minecraftforge.fml.relauncher.Side;
import net.minecraftforge.fml.relauncher.SideOnly;
import pub.campfire.auth.network.AuthResponseMessage;

/**
 * The client-only half of the handshake, held behind @SideOnly(Side.CLIENT)
 * so this class is never loaded on the dedicated server. Reads the two JVM
 * system properties the launcher (or, before it exists, a hand-launched
 * client) sets — no dependency on the player entity existing, which is why
 * this design never touches the client connect event (RESEARCH.md
 * Pitfall 1).
 */
@SideOnly(Side.CLIENT)
public final class ClientAuthHandler {
    private ClientAuthHandler() {
    }

    public static AuthResponseMessage buildResponse() {
        String nick = System.getProperty("campfire.nick", "");
        String token = System.getProperty("campfire.token", "");
        return new AuthResponseMessage(nick, token);
    }
}
