package pub.campfire.auth.network;

import io.netty.buffer.ByteBuf;
import net.minecraft.network.PacketBuffer;
import net.minecraftforge.fml.common.FMLCommonHandler;
import net.minecraftforge.fml.common.network.ByteBufUtils;
import net.minecraftforge.fml.common.network.simpleimpl.IMessage;
import net.minecraftforge.fml.common.network.simpleimpl.IMessageHandler;
import net.minecraftforge.fml.common.network.simpleimpl.MessageContext;
import pub.campfire.auth.server.ServerAuthHandler;

/**
 * Client-to-server: the two strings the client mod read from -D properties.
 * This is the only place in the mod where fully attacker-controlled bytes
 * are parsed, so both fields are length-bounded at 256 characters — an
 * over-long field invalidates the message (fields set to empty string)
 * rather than allocating whatever length the client claims.
 */
public class AuthResponseMessage implements IMessage {
    private static final int MAX_FIELD_LENGTH = 256;

    private String nick;
    private String token;

    public AuthResponseMessage() {
    }

    public AuthResponseMessage(String nick, String token) {
        this.nick = nick == null ? "" : nick;
        this.token = token == null ? "" : token;
    }

    public String getNick() {
        return nick;
    }

    public String getToken() {
        return token;
    }

    @Override
    public void toBytes(ByteBuf buf) {
        ByteBufUtils.writeUTF8String(buf, nick);
        ByteBufUtils.writeUTF8String(buf, token);
    }

    @Override
    public void fromBytes(ByteBuf buf) {
        nick = readBoundedUtf8(buf);
        token = readBoundedUtf8(buf);
    }

    private static String readBoundedUtf8(ByteBuf buf) {
        // PacketBuffer's own bounded-string reader rejects (and disconnects
        // the sender on) any declared length over the cap, before it ever
        // allocates a buffer for the claimed length.
        try {
            return new PacketBuffer(buf).readString(MAX_FIELD_LENGTH);
        } catch (Exception e) {
            return "";
        }
    }

    /**
     * Runs on the network thread (Pattern 3) — does no game-state work
     * here. Hands off to the main thread via addScheduledTask and returns
     * null (no reply expected for this message).
     */
    public static class Handler implements IMessageHandler<AuthResponseMessage, IMessage> {
        @Override
        public IMessage onMessage(AuthResponseMessage message, MessageContext ctx) {
            String nick = message.getNick();
            String token = message.getToken();
            FMLCommonHandler.instance().getMinecraftServerInstance().addScheduledTask(() ->
                    ServerAuthHandler.onResponseReceived(ctx.getServerHandler().player, nick, token));
            return null;
        }
    }
}
