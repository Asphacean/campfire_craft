package pub.campfire.auth;

import net.minecraftforge.fml.common.Mod;
import net.minecraftforge.fml.common.event.FMLPreInitializationEvent;
import pub.campfire.auth.network.NetworkHandler;

/**
 * Entry point. acceptableRemoteVersions is set to accept any remote version
 * (including a client with no mod at all) so Forge's own handshake never
 * rejects the connection before ServerAuthHandler gets a chance to run —
 * without this, a tokenless client sees a generic Forge mod-mismatch screen
 * instead of the bilingual instruction to use the launcher (RESEARCH.md
 * "Anti-Patterns" / this plan's D-06..D-10 read).
 */
@Mod(modid = CampfireAuth.MODID, name = CampfireAuth.NAME, version = CampfireAuth.VERSION,
        acceptedMinecraftVersions = "[1.12.2]",
        acceptableRemoteVersions = "*")
public class CampfireAuth {
    public static final String MODID = "campfireauth";
    public static final String NAME = "Campfire Auth";
    public static final String VERSION = "0.1.0";

    @Mod.EventHandler
    public void preInit(FMLPreInitializationEvent event) {
        NetworkHandler.init();
    }
}
