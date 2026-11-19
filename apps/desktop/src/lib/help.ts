/** Plain-language help for the "?" tips. Keep wording non-technical and honest. */
export type HelpTopic = {
  title: string;
  what: string;
  why: string;
  example: string;
  /** Honest pros and cons: who can still see what. */
  tradeoff: string;
  /** For choices: what each option does, so the reader can pick. */
  options?: { name: string; text: string }[];
  /** A plain recommendation for most people. */
  pick?: string;
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
    what: "A quick check, run automatically a few seconds after you connect, that nothing is escaping the VPN: the kill switch, DNS lookups, IPv6, whether the server is reachable, another VPN app running at the same time, and browsers that use their own DNS.",
    why: "A VPN that looks connected can still leak. This shows you instead of leaving you to trust it.",
    example:
      "It runs by itself after you connect; press Test again to rerun it. Green checks mean no leak was found. A red or yellow one says what to fix, such as quitting Cloudflare WARP or turning off Chrome's secure DNS.",
    tradeoff:
      "If another VPN is on, the Stop it button disconnects it (Cloudflare WARP) or asks it to quit, like pressing Quit yourself. It never force-closes anything, and only acts when you press it. You can reconnect that VPN later. It checks this device's setup, not what the VPN provider logs. A pass means no leak was found now, not a guarantee for later.",
  },
  helperSetup: {
    title: "One-time Mac setup",
    what: "VPNFlow needs a small background helper on your Mac to create the VPN tunnel and the kill switch. Setup installs it once and asks for your Mac password, the same one you use to install apps.",
    why: "macOS only lets a program with administrator rights change network settings. Without the helper, Connect stays off.",
    example: "Press Set up, type your Mac password, and VPNFlow restarts by itself, ready to connect.",
    tradeoff:
      "It installs two files in /usr/local/libexec/vpnflow and starts them in the background. VPNFlow checks both files are exactly the ones inside the app before it starts them. Nothing is sent anywhere. If you press Cancel, nothing changes. We recommend setting it up: it is required to connect. Remove it any time with scripts/macos/install-daemon.sh uninstall.",
  },
  speedTest: {
    title: "Connection speed",
    what: "Measures how fast data moves through the VPN right now. By default it runs once on its own, a few seconds after each connection is stable, and shows the result in Connection details.",
    why: "Servers differ in speed. This helps you decide whether to switch location.",
    example: "If downloads feel slow, test, then try another location and test again to compare.",
    tradeoff:
      "Any VPN is slower than your direct connection because traffic takes a detour and is encrypted. Each test uses about 30 MB and contacts Cloudflare's speed server (no identifiers sent). Recommendation: leave it on to compare servers; turn it off in Settings if you're on a metered connection.",
  },
  quickConnect: {
    title: "Quick Connect",
    what: "Connects you to a good server without picking one.",
    why: "It prefers servers that answered recently, then your preferred or favourite profile, then the last one you used. It does not measure speed first.",
    example: "Press it when you just want to be protected and don't care which country.",
    tradeoff:
      "It doesn't measure speed, so the server it picks may not be the fastest. Choose a location yourself if you need a specific country.",
  },
  trustedNetworks: {
    title: "Trusted networks",
    what: "Marks networks you trust, like your home Wi-Fi. VPNFlow turns the VPN off on them and turns it on everywhere else.",
    why: "At home you may not need the VPN, and it can slow things down or block devices like printers. On cafe, hotel or airport Wi-Fi you do want it.",
    example:
      "Trust your home network once. When you join it, the VPN disconnects. When you join a cafe network, VPNFlow connects for you.",
    tradeoff:
      "A network is recognised by its router, so someone could in theory copy your router's address on a fake network and make VPNFlow switch off. On a trusted network your internet provider and the sites you visit see your real address and traffic. It acts only when the network changes, so connecting by hand stays as you left it.",
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
  dns: {
    title: "DNS and ad blocking",
    what: "Every time you open a site, your computer first asks a DNS service to turn the name (like news.com) into an address. This setting chooses who answers.",
    why: "A blocking service refuses to answer for known ad, tracker or dangerous sites, so they never load. That works on every app, with nothing to install.",
    example:
      "With AdGuard, a page that normally loads ten ad trackers loads without most of them, and pages often load a little faster.",
    tradeoff:
      "The service you pick sees the names of the sites you look up, instead of the VPN provider. It cannot see page contents. DNS blocking misses some ads (YouTube and ads served from the site's own address). If a site breaks, switch back to the VPN's own. Applies from the next connection.",
    options: [
      {
        name: "The VPN's own",
        text: "Proton answers. Most private (one company sees you), no blocking.",
      },
      {
        name: "AdGuard",
        text: "Blocks ads, trackers and many scam sites. Most noticeable change.",
      },
      {
        name: "Cloudflare malware",
        text: "Blocks only known virus and scam sites. Very rarely breaks anything.",
      },
      {
        name: "Quad9",
        text: "Blocks phishing and malware, run by a non-profit in Switzerland. No ad blocking.",
      },
      {
        name: "Cloudflare",
        text: "Fast, no blocking. Only for speed.",
      },
      {
        name: "Your own servers",
        text: "For advanced setups, such as a service you pay for. Public IPv4 addresses only.",
      },
    ],
    pick: "For most people: AdGuard to cut ads and trackers, or the VPN's own if you want the simplest, most private setup. We have not measured speed on your connection, so there is no per-machine recommendation yet.",
  },
  schedule: {
    title: "Schedule",
    what: "Connects and disconnects the VPN by itself at the times and days you choose.",
    why: "Useful for a routine, like protection during work hours, without remembering to switch it on.",
    example: "Connect at 09:00 and disconnect at 17:00 on Monday to Friday.",
    tradeoff:
      "It acts only at the exact minute. If your Mac is asleep or VPNFlow is closed then, that time is skipped, not made up later. It never overrides what you do by hand in between. Uses your computer's clock and time zone.",
  },
  autoReconnect: {
    title: "Reconnect automatically",
    what: "Tries to bring the VPN back by itself when it drops.",
    why: "Saves you noticing and reconnecting by hand after a network change.",
    example: "You close your laptop and open it on another Wi-Fi. VPNFlow reconnects on its own.",
    tradeoff:
      "While it retries, you may be briefly unprotected unless the kill switch is on. Pair the two for full coverage.",
  },
  autoSwitchServer: {
    title: "Switch server automatically",
    what: "If your server stops responding, VPNFlow tries up to three other servers in the same country, one at a time.",
    why: "Free servers go down or fill up. This keeps you connected without you doing anything, and you stay in the same country.",
    example:
      "Canada #1 stops answering. VPNFlow moves you to Canada #15 and logs it in Activity. If every Canada server fails, it tells you to choose another country in Locations.",
    tradeoff:
      "It only reacts to servers that stop responding, not ones that are merely slow, because the app can't measure speed or load. Your exit address changes when it switches. Recommendation: leave it on. With the kill switch on, nothing leaks while it switches.",
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
  locations: {
    title: "VPN locations",
    what: "Your list of servers, grouped by country. Each server is one WireGuard profile you imported or added.",
    why: "Picking a country changes where websites think you are, and some servers are faster or more reliable than others.",
    example: "Choose Japan, then Connect. Sites now see a Japanese address instead of yours.",
    tradeoff:
      "VPNFlow only connects to servers you add. The country on a server is the label you or the file gave it, and VPNFlow does not check it is true.",
  },
  locationFilters: {
    title: "Find and refresh servers",
    what: "Search and sort the one server list. Starred servers sit on top, and Recently used below the list shows where you connected before. Refresh status updates what the app knows about the one you are connected to.",
    why: "One list is quicker to scan than separate tabs. Status tells you which servers answered recently.",
    example: "Star a server and it moves to the top of the list.",
    tradeoff:
      "Idle servers are never contacted just to check them, so a server that shows no response is not necessarily down, it just has not been used lately.",
    options: [
      { name: "Favorites first", text: "Starred servers and countries on top. This is the default." },
      { name: "Country name", text: "Alphabetical by country." },
      { name: "Recently connected", text: "Servers you connected to most recently first." },
    ],
    pick: "Leave it on Favorites first. Star the servers you use most.",
  },
  quickConnectCountry: {
    title: "Quick Connect country",
    what: "Limits Quick Connect to servers in one country.",
    why: "So the one-click button always puts you where you want, such as a country with the best streaming or the nearest servers.",
    example: "Set Japan. Quick Connect now only picks among your Japan servers.",
    tradeoff:
      "If the country you pick has no working server, Quick Connect has nothing to choose and says so. It does not fall back to another country.",
    options: [
      { name: "Any country", text: "Quick Connect picks the best server it knows about anywhere." },
      { name: "A specific country", text: "Quick Connect picks only from that country." },
    ],
    pick: "Any country for most people. Choose one only if you always want to appear in the same place.",
  },
  serverDetails: {
    title: "Server details",
    what: "The label and switches for one server: its name, country, city, whether it is a favorite, and whether VPNFlow may connect to it.",
    why: "Good labels make the list easy to search, and turning a server off keeps it saved without using it.",
    example: "Rename a server to Tokyo home, mark it favorite, and it shows first under Favorites.",
    tradeoff:
      "Country, city and time zone are only labels you set. They do not change where the server really is. Turning off Enable connection attempts stops Connect and Quick Connect from using it. Some servers need you to register your key with the server owner before they will accept you.",
  },
  checks: {
    title: "Checks",
    what: "A fresh set of tests of this computer's VPN setup: kill switch, DNS, IPv6, internet, your public address, other VPN apps and browser DNS.",
    why: "It shows what is protected right now and what to fix, instead of leaving you to trust the green connected light.",
    example: "A yellow row such as Other VPN apps tells you what is running and how to stop it.",
    tradeoff:
      "These run on this computer and contact no outside site. They show your setup now, not a promise for later, so run them again after changing something.",
  },
  events: {
    title: "Events",
    what: "A running log of what VPNFlow did: connecting, disconnecting, reconnecting, and anything that went wrong.",
    why: "When something breaks, this tells you what happened and when, in order.",
    example: "Seeing Reconnecting at 10:02 and Connected at 10:03 explains a short internet pause.",
    tradeoff:
      "It is kept on this computer only. It lists server names and times, not what sites you visited. Switch to Warnings and errors to hide routine messages.",
    options: [
      { name: "All", text: "Every event, including normal ones." },
      { name: "Warnings and errors", text: "Only problems." },
    ],
    pick: "Use Warnings and errors when hunting a problem, All when you need the full order of events.",
  },
  exportDiagnostics: {
    title: "Export diagnostics",
    what: "Saves the checks and recent events to a file you can send to someone who is helping you.",
    why: "It gives a helper the facts without you describing them from memory.",
    example: "Export, then attach the file to a support request or an issue report.",
    tradeoff:
      "The file contains public keys, server addresses and these events. It never contains private or preshared keys. Read it first if the server addresses are sensitive, since it is saved only where you choose.",
  },
} satisfies Record<string, HelpTopic>;

export type HelpKey = keyof typeof HELP;
