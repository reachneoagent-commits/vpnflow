# Recovery

1. Open Dashboard or the tray. Use **Disconnect** if the tunnel may be up,
   or **Stop blocking** if a session ended and the firewall remains active.
   Authenticate the Polkit request to remove the block.
2. If the app cannot run, an authorized administrator at the console can
   inspect the Aegis block and remove exactly its owned table using the installed
   helper. Run `pkexec /usr/libexec/aegis-vpn/aegis-helper status`, then
   `pkexec /usr/libexec/aegis-vpn/aegis-helper remove` when ready to allow the
   normal connection. Retry if cleanup reports an error. Keep an independent
   way to reach the machine before changing its firewall.
3. If NetworkManager still reports an active Aegis profile, use the desktop's
   network settings to disconnect that specific profile. Preserve unrelated
   connections and firewall tables.
4. If local metadata goes missing, restore the registry backup or inspect the
   system profiles. Startup preserves backend profiles absent from the local
   registry. Keep an original protected copy of your WireGuard configuration.
5. DEB/RPM package removal runs helper cleanup before removing its recovery
   components. If cleanup fails, fix the reported issue and retry removal.
   An upgrade retains the existing block. To uninstall a source-installed helper, use
   `sudo scripts/install-helper.sh uninstall`. It keeps the helper and policy
   available if firewall cleanup fails. Remove the desktop package using the
   package manager after disconnecting and removing any remaining Aegis block.

After recovery, verify original routes and DNS with the operating-system tools.
Gather Activity diagnostics for support and review endpoints and other local
metadata before sharing. Keep WireGuard configuration files private.
