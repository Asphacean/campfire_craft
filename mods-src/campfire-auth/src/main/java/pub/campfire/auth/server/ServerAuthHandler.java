package pub.campfire.auth.server;

import net.minecraft.entity.player.EntityPlayerMP;
import net.minecraft.util.text.TextComponentString;
import net.minecraft.world.GameType;
import net.minecraftforge.event.CommandEvent;
import net.minecraftforge.event.ServerChatEvent;
import net.minecraftforge.fml.common.FMLCommonHandler;
import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.common.eventhandler.SubscribeEvent;
import net.minecraftforge.fml.common.gameevent.PlayerEvent;
import net.minecraftforge.fml.common.gameevent.TickEvent;
import net.minecraftforge.fml.relauncher.Side;
import org.apache.logging.log4j.LogManager;
import org.apache.logging.log4j.Logger;
import pub.campfire.auth.CampfireAuth;
import pub.campfire.auth.network.AuthRequestMessage;
import pub.campfire.auth.network.NetworkHandler;

import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;

/**
 * The enforcement point. Every join is frozen (spectator game type) the
 * instant it completes, a request packet is sent, and the player is
 * released only on an explicit HTTP 200 from campfire-auth's /validate.
 * Every other outcome — no packet within 5s, a nick that doesn't match the
 * connection's own profile name, a non-200 answer, a timeout, a connection
 * refusal — ends in a kick. The result variable in the HTTP callback starts
 * false and only an explicit 200 ever sets it true (T-02-02-01): this is
 * the single load-bearing security property of the whole project.
 */
@Mod.EventBusSubscriber(value = Side.SERVER, modid = CampfireAuth.MODID)
public final class ServerAuthHandler {
    private static final Logger LOGGER = LogManager.getLogger(CampfireAuth.MODID);
    private static final String VALIDATE_URL = "http://127.0.0.1:8081/validate";
    private static final long TIMEOUT_MILLIS = 5000L;
    private static final int CONNECT_TIMEOUT_MILLIS = 3000;
    private static final int READ_TIMEOUT_MILLIS = 3000;
    private static final String KICK_MESSAGE =
            "Зайди через лаунчер campfire.pub / Join via the campfire.pub launcher";

    private static final Map<UUID, PendingJoin> PENDING = new HashMap<>();

    private ServerAuthHandler() {
    }

    @SubscribeEvent
    public static void onPlayerLoggedIn(PlayerEvent.PlayerLoggedInEvent event) {
        if (!(event.player instanceof EntityPlayerMP)) {
            return;
        }
        EntityPlayerMP player = (EntityPlayerMP) event.player;
        UUID uuid = player.getUniqueID();
        String nick = player.getGameProfile().getName();

        GameType previousGameType = player.interactionManager.getGameType();
        PendingJoin pending = new PendingJoin(
                System.currentTimeMillis(), previousGameType,
                player.posX, player.posY, player.posZ,
                player.rotationYaw, player.rotationPitch);
        PENDING.put(uuid, pending);

        player.setGameType(GameType.SPECTATOR);
        NetworkHandler.INSTANCE.sendTo(new AuthRequestMessage(), player);
        LOGGER.info("[campfireauth] nick={} join=pending", nick);
    }

    @SubscribeEvent
    public static void onPlayerLoggedOut(PlayerEvent.PlayerLoggedOutEvent event) {
        if (event.player == null) {
            return;
        }
        // A disconnect during validation must not leak a pending entry or
        // resurrect a stale freeze for a later reconnect.
        PENDING.remove(event.player.getUniqueID());
    }

    @SubscribeEvent
    public static void onServerChat(ServerChatEvent event) {
        if (PENDING.containsKey(event.getPlayer().getUniqueID())) {
            event.setCanceled(true);
        }
    }

    @SubscribeEvent
    public static void onCommand(CommandEvent event) {
        if (event.getSender() instanceof EntityPlayerMP) {
            EntityPlayerMP player = (EntityPlayerMP) event.getSender();
            if (PENDING.containsKey(player.getUniqueID())) {
                event.setCanceled(true);
            }
        }
    }

    @SubscribeEvent
    public static void onServerTick(TickEvent.ServerTickEvent event) {
        if (event.phase != TickEvent.Phase.END || PENDING.isEmpty()) {
            return;
        }
        long now = System.currentTimeMillis();
        java.util.Iterator<Map.Entry<UUID, PendingJoin>> it = PENDING.entrySet().iterator();
        while (it.hasNext()) {
            Map.Entry<UUID, PendingJoin> entry = it.next();
            if (now - entry.getValue().joinTimeMillis > TIMEOUT_MILLIS) {
                UUID uuid = entry.getKey();
                it.remove();
                EntityPlayerMP player = FMLCommonHandler.instance().getMinecraftServerInstance()
                        .getPlayerList().getPlayerByUUID(uuid);
                if (player != null) {
                    kick(player, "no_packet");
                }
            }
        }
    }

    /**
     * Called (already on the main thread — the caller schedules this via
     * addScheduledTask) when the client's AuthResponseMessage arrives.
     * The packet's nick field is client-supplied and is never treated as
     * identity: it is compared only against the connection's own profile
     * name, and only the token is forwarded to /validate.
     */
    public static void onResponseReceived(EntityPlayerMP player, String claimedNick, String token) {
        if (player == null) {
            return;
        }
        UUID uuid = player.getUniqueID();
        PendingJoin pending = PENDING.get(uuid);
        if (pending == null || pending.validating) {
            // Already resolved (timed out or a duplicate/late packet), or a
            // validation call is already in flight for this join (WR-02) —
            // ignore. Without this guard an unbounded number of response
            // packets during the pending window could each spawn their own
            // HTTP validate call and race each other to resolve the join.
            return;
        }

        String ownNick = player.getGameProfile().getName();
        if (!ownNick.equals(claimedNick)) {
            PENDING.remove(uuid);
            kick(player, "nick_mismatch");
            return;
        }

        pending.validating = true;
        validateAsync(ownNick, token, uuid);
    }

    private static void validateAsync(String nick, String token, UUID uuid) {
        new Thread(() -> {
            boolean valid;
            String failureReason;
            try {
                URL url = new URL(VALIDATE_URL);
                HttpURLConnection conn = (HttpURLConnection) url.openConnection();
                conn.setRequestMethod("POST");
                conn.setRequestProperty("Content-Type", "application/json");
                conn.setDoOutput(true);
                conn.setConnectTimeout(CONNECT_TIMEOUT_MILLIS);
                conn.setReadTimeout(READ_TIMEOUT_MILLIS);

                String body = "{\"nick\":\"" + jsonEscape(nick) + "\",\"token\":\"" + jsonEscape(token) + "\"}";
                try (OutputStream os = conn.getOutputStream()) {
                    os.write(body.getBytes(StandardCharsets.UTF_8));
                }

                int status = conn.getResponseCode();
                valid = status == 200;
                failureReason = valid ? null : "invalid_token";
                // WR-05: fully drain and close the response so
                // HttpURLConnection can return this connection to its
                // keep-alive pool instead of opening a fresh socket per
                // join (or leaking one under sustained load).
                try (java.io.InputStream is = valid ? conn.getInputStream() : conn.getErrorStream()) {
                    if (is != null) {
                        while (is.read() != -1) {
                            // drain
                        }
                    }
                } finally {
                    conn.disconnect();
                }
            } catch (Exception e) {
                valid = false;
                failureReason = "service_error";
            }

            boolean finalValid = valid;
            String finalReason = failureReason;
            FMLCommonHandler.instance().getMinecraftServerInstance().addScheduledTask(() ->
                    applyValidationResult(uuid, nick, finalValid, finalReason));
        }, "campfireauth-validate").start();
    }

    private static void applyValidationResult(UUID uuid, String nick, boolean valid, String failureReason) {
        PendingJoin pending = PENDING.remove(uuid);
        if (pending == null) {
            // Timed out and already kicked while the HTTP call was in flight.
            return;
        }
        EntityPlayerMP player = FMLCommonHandler.instance().getMinecraftServerInstance()
                .getPlayerList().getPlayerByUUID(uuid);
        if (player == null) {
            // Disconnected while the HTTP call was in flight.
            return;
        }
        if (valid) {
            player.setGameType(pending.previousGameType);
            player.connection.setPlayerLocation(pending.posX, pending.posY, pending.posZ,
                    pending.yaw, pending.pitch);
            LOGGER.info("[campfireauth] nick={} result=allow", nick);
        } else {
            LOGGER.info("[campfireauth] nick={} result=kick reason={}", nick, failureReason);
            player.connection.disconnect(new TextComponentString(KICK_MESSAGE));
        }
    }

    private static void kick(EntityPlayerMP player, String reason) {
        LOGGER.info("[campfireauth] nick={} result=kick reason={}", player.getGameProfile().getName(), reason);
        player.connection.disconnect(new TextComponentString(KICK_MESSAGE));
    }

    private static String jsonEscape(String s) {
        StringBuilder sb = new StringBuilder(s.length());
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"':
                    sb.append("\\\"");
                    break;
                case '\\':
                    sb.append("\\\\");
                    break;
                case '\n':
                    sb.append("\\n");
                    break;
                case '\r':
                    sb.append("\\r");
                    break;
                case '\t':
                    sb.append("\\t");
                    break;
                default:
                    if (c < 0x20) {
                        sb.append(String.format("\\u%04x", (int) c));
                    } else {
                        sb.append(c);
                    }
            }
        }
        return sb.toString();
    }

    private static final class PendingJoin {
        final long joinTimeMillis;
        final GameType previousGameType;
        final double posX, posY, posZ;
        final float yaw, pitch;
        // WR-02: set once the first AuthResponseMessage is accepted, so any
        // further packet for this join is ignored rather than spawning a
        // second, racing validate call. Only ever read/written on the main
        // thread (all PENDING access is), so no synchronization is needed.
        boolean validating = false;

        PendingJoin(long joinTimeMillis, GameType previousGameType,
                    double posX, double posY, double posZ, float yaw, float pitch) {
            this.joinTimeMillis = joinTimeMillis;
            this.previousGameType = previousGameType;
            this.posX = posX;
            this.posY = posY;
            this.posZ = posZ;
            this.yaw = yaw;
            this.pitch = pitch;
        }
    }
}
