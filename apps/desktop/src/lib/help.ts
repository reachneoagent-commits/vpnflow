/** Plain-language help for the "?" tips. Keep wording non-technical and honest. */
export type HelpTopic = {
  title: string;
  what: string;
  why: string;
  example: string;
  /** Honest pros and cons: who can still see what. */
  tradeoff: string;
};

export const HELP = {
  killSwitch: {
    title: "Kill switch",
    what: "A safety net that blocks your internet if the VPN connection drops.",
    why: "Without it, a dropped VPN can quietly send your traffic over your normal connection, showing your real address.",
    example:
      "Your Wi-Fi hiccups and the VPN disconnects. With the kill switch on, websites stop loading instead of loading without protection.",
    tradeoff:
      "If the VPN drops, you lose internet until it reconnects or you press Disconnect. It stops leaks but does not hide anything from the VPN provider, who can still see your traffic's destinations.",
  },
  allowLan: {
    title: "Allow local network",
    what: "Lets devices in your home or office stay reachable while the kill switch is blocking.",
    why: "The kill switch blocks everything outside the VPN, which can also cut off your printer or file server.",
    example:
      "Printing to the office printer or opening a shared folder still works while the kill switch is blocking.",
    tradeoff:
      "Devices on your local network can see and reach your computer outside the VPN. On a shared or public Wi-Fi, leave this off.",
  },
  dnsProtection: {
    title: "DNS leak protection",
    what: "DNS is the lookup that turns a name like example.com into an address. This keeps those lookups inside the VPN.",
    why: "If lookups go to your internet provider instead, they can still see which sites you visit, even though the pages themselves are protected.",
    example: "You open a news site. The lookup goes to the VPN's servers, not your internet provider.",
    tradeoff:
      "The VPN provider's DNS now sees your lookups instead of your internet provider. Your provider can still see that you are connected to a VPN server, just not which sites you look up.",
  },
  ipv6Protection: {
    title: "IPv6 protection",
    what: "IPv6 is a newer kind of internet address. This makes sure it cannot bypass the VPN.",
    why: "Some VPNs only cover the older addresses, so IPv6 traffic can leak out with your real address.",
    example: "A site that supports IPv6 is reached through the VPN, or IPv6 is blocked while connected.",
    tradeoff:
      "If a profile has no IPv6, IPv6 is blocked while connected, so a few IPv6-only sites may not load.",
  },
  leakTest: {
    title: "Leak test",
    what: "A quick check that nothing is escaping the VPN: the kill switch, DNS lookups, IPv6 and whether the server is reachable.",
    why: "A VPN that looks connected can still leak. This shows you instead of leaving you to trust it.",
    example: "Run it after connecting. Green checks mean no leak was found. A red one says what to fix.",
    tradeoff:
      "It checks this device's setup, not what the VPN provider logs. A pass means no leak was found now, not a guarantee for later.",
  },
  speedTest: {
    title: "Connection speed",
    what: "Measures how fast data moves through the VPN right now.",
    why: "Servers differ in speed. This helps you decide whether to switch location.",
    example: "If downloads feel slow, test, then try another location and test again to compare.",
    tradeoff:
      "Any VPN is slower than your direct connection because traffic takes a detour and is encrypted. The test uses some of your data.",
  },
  quickConnect: {
    title: "Quick Connect",
    what: "Connects you to a good server without picking one.",
    why: "It prefers servers that answered recently, then your preferred or favourite profile, then the last one you used. It does not measure speed first.",
    example: "Press it when you just want to be protected and don't care which country.",
    tradeoff:
      "It doesn't measure speed, so the server it picks may not be the fastest. Choose a location yourself if you need a specific country.",
  },
  bypass: {
    title: "Skip the VPN",
    what: "A list of addresses that use your normal internet connection instead of the VPN.",
    why: "Some devices or services don't work through a VPN, or you don't need them protected.",
    example:
      "Add 192.168.1.20 for a home server, or 192.168.1.0/24 for a whole home network. Those skip the VPN; everything else stays protected. Skipped traffic is not encrypted by VPNFlow.",
    tradeoff:
      "Traffic to these addresses is not encrypted by the VPN. Your internet provider and the destination can see it, and it shows your real address. DNS lookups for skipped names may still go through the VPN. Only skip devices you trust, such as ones on your home network.",
  },
  autoReconnect: {
    title: "Reconnect automatically",
    what: "Tries to bring the VPN back by itself when it drops.",
    why: "Saves you noticing and reconnecting by hand after a network change.",
    example: "You close your laptop and open it on another Wi-Fi. VPNFlow reconnects on its own.",
    tradeoff:
      "While it retries, you may be briefly unprotected unless the kill switch is on. Pair the two for full coverage.",
  },
  preferredProfile: {
    title: "Preferred profile",
    what: "Your default server.",
    why: "The tray's Connect item and Connect on launch use it.",
    example: "Pick a nearby country so one click always connects you there.",
    tradeoff:
      "It is always the same server, so you appear in the same place every time, which can make you easier to link across sessions.",
  },
  connectOnLaunch: {
    title: "Connect on launch",
    what: "Connects your preferred profile as soon as VPNFlow starts.",
    why: "So you are protected without remembering to connect.",
    example: "Turn on with Start VPNFlow at sign-in and you are protected as soon as you log in.",
    tradeoff:
      "It adds a few seconds at start-up and uses the preferred profile even if you wanted another country.",
  },
  connectivityCheck: {
    title: "Connectivity check",
    what: "A tiny lookup, at most once a minute, to confirm the VPN really reaches the internet.",
    why: "It's how the app knows the connection works, and it provides your exit address.",
    example: "It asks the VPN's own DNS server for example.com. Nothing is sent to any other company.",
    tradeoff:
      "The VPN's DNS server sees those lookups. Turning it off removes them, but the dashboard can no longer confirm that the internet is reachable.",
  },
  closeToTray: {
    title: "Keep in the tray",
    what: "Closing the window hides VPNFlow instead of quitting it.",
    why: "The VPN keeps running and you can control it from the menu bar.",
    example: "Close the window; the icon stays in the menu bar and the VPN stays connected.",
    tradeoff:
      "VPNFlow keeps running in the background. Choose Quit from the menu bar to close it fully; the VPN stays as it is.",
  },
} satisfies Record<string, HelpTopic>;

export type HelpKey = keyof typeof HELP;
