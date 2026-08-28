package pub.campfire.auth.network;

import io.netty.buffer.ByteBuf;
import net.minecraftforge.fml.common.network.simpleimpl.IMessage;
import net.minecraftforge.fml.common.network.simpleimpl.IMessageHandler;
import net.minecraftforge.fml.common.network.simpleimpl.MessageContext;
import pub.campfire.auth.client.ClientAuthHandler;

/**
 * Server-to-client, empty payload — sent the instant PlayerLoggedInEvent
 * fires (Pattern 1: server-initiated handshake). The client's handler is
 * registered below via an inner class so it can live in the same file as
 * the message it handles without a hidden Side.CLIENT reference in the
 * message class body — the handler itself is Side.CLIENT via
 * NetworkHandler's registerMessage call, never loaded on a dedicated server.
 */
public class AuthRequestMessage implements IMessage {
    public AuthRequestMessage() {
    }

    @Override
    public void toBytes(ByteBuf buf) {
    }

    @Override
    public void fromBytes(ByteBuf buf) {
    }

    /**
     * Delegates to ClientAuthHandler (@SideOnly(Side.CLIENT)) and replies
     * synchronously. This Handler class is itself only ever loaded on the
     * client (registered for Side.CLIENT in NetworkHandler), but the
     * property-reading logic lives behind its own @SideOnly boundary so it
     * is never even reachable from a dedicated-server classload.
     * Deliberately does NOT touch Minecraft.getMinecraft().player or any
     * client world state — that is precisely why this design uses a
     * server-initiated request instead of a client-side connect-event
     * listener (RESEARCH.md Pitfall 1: the player entity can be null when
     * a connect event fires, but a system-property read has no such
     * dependency and is always safe).
     */
    public static class Handler implements IMessageHandler<AuthRequestMessage, AuthResponseMessage> {
        @Override
        public AuthResponseMessage onMessage(AuthRequestMessage message, MessageContext ctx) {
            return ClientAuthHandler.buildResponse();
        }
    }
}
