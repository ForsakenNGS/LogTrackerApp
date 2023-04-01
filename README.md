![Banner](https://github.com/ForsakenNGS/LogTrackerApp/blob/master/doc_resources/images/banner/LogTrackerBanner.png?raw=true)
![GitHub all releases](https://img.shields.io/github/downloads/ForsakenNGS/LogTrackerApp/total?label=Downloads) ![GitHub issues](https://img.shields.io/github/issues-raw/ForsakenNGS/LogTrackerApp?label=Open%20Issues) ![GitHub](https://img.shields.io/github/license/ForsakenNGS/LogTrackerApp?label=License)

# Disclaimer!

**This application may offend certain points of [WarcraftLogs API terms of service](https://articles.classic.warcraftlogs.com/help/rpg-logs-api-terms-of-service). Use it at your own risk!**

**I will only support this way of obtaining log data until there is an official alternative provided by WarcraftLogs!**

For clarification I will explicitly address the most critical points here:

> Unless expressly permitted by the content owner or by applicable law, you will not, and will not permit your end users or others acting on your behalf to, do the following with content returned from the APIs:
>
> 1. Scrape, build databases, or otherwise create permanent copies of such content, or keep cached copies longer than permitted by the cache header;

The app will only download log data for players where needed, limiting requests to players you actually encounter in game. (Mouseover/Tooltip and LFG-Tool) It will not "scrape all there is" and limits requests done as much as possible. It will only be stored partially with a limited lifespan.

> 2. Present content substantially unchanged through a new channel, including but not limited to: a competing website, in-game add-ons, game overlays, social platforms, or mobile platforms;
>
> 3. Copy, translate, modify, create a derivative work of, sell, lease, lend, convey, distribute, publicly display, or sublicense to any third party;

If the according sync option (Escape > Interface > Addons > LogTracker > Send player data to other clients) is disabled, the data you obtain via the API will only be shown to yourself. If it is enabled it will to some extend "distribute and/or publicly display" the data, but reduce the amount of requests needed to obtain a helpful amount of information.

Taken literally, every site using the API (including e.g. ironforge.pro) violates these points.

> Misrepresent the source or ownership; or
>
> Remove, obscure, or alter any copyright, trademark, or other proprietary rights notices; or falsify or delete any author attributions, legal notices, or other labels of the origin or source of material.

Upon login/zone change the Addon will clearly communicate ownership of the data and even endorse supporting WarcraftLogs.

**Installation and Usage**
---
![App Image](https://github.com/ForsakenNGS/LogTrackerApp/blob/master/doc_resources/images/app/LogTrackerApp_1.PNG?raw=true)
1. Section
    + Add the game directory. This should point towards your "\_classic\_" folder
    + Add your [WarcraftLogs API](https://classic.warcraftlogs.com/api/clients) credentials.
        + See [WarcraftLogs API Section](https://github.com/ForsakenNGS/LogTrackerApp#warcraftlogs-api) in down below
2. Section
	+ Here you can manually update a player if you want to. Just enter the realm and name and click the "Update" button.
3. Section
    + Priority / Regular
        + This shows the number of pending updates for the regular- and priority-queue
    + Update X/X
        + This shows the current queue of player to be updated.
    + Reserving X
        + This reserves API-Points for manual updates.
        + These are used up by the queue before the next reset in order not to waste them.
    
**Mode of Operation**
---
This application works in conjunction with the [LogTrackler WoW Addon](https://github.com/ForsakenNGS/LogTracker).

The addon adds players it meets in-game to a list. Then application then takes this list and pulls the logs via the official WarcraftLogs API.

Then the information is fed back to the addon.

The addon then displays the information in-game. The addon also distributes this information to others with the same addon.

**Group Finder usage**
---
The addon integrates into the group finder and will automatically prioritize updating listed players.
You can also limit the addon to only update those + members of your guild / raid-groups with the "Only perform prioritized updates via App" option. (Escape > Interface > Addons > LogTracker)

The usual workflow is the following:
- Open the Group Finder and select the desired raid(s)
- Wait a few seconds and `/reload` your interface
- Wait until the priority updates in the app are done (See **Screenshot 1** below)
- `/reload` your interface again
- Open the Group Finder again (if you are not listed, you also have to select the raid(s) again)
- Repeat as nescessary (As seen in **Screenshot 2** below, you can judge this by the status report within the group finder)

This approach, while certainly not perfect, allows a more efficient use of the available API quota.
Especially if your server don't have a lot of players running the app, this will be a lot more useful in the short term.

 **Screenshot 1**

![Update status within the app](https://github.com/ForsakenNGS/LogTrackerApp/blob/master/doc_resources/images/app/LogTrackerApp_2.PNG?raw=true)

 **Screenshot 2**

![Queue prediction in-game](https://github.com/ForsakenNGS/LogTrackerApp/blob/master/doc_resources/images/game/GroupFinder_1.PNG?raw=true)

**WarcraftLogs API**
---
When logged in on WarcraftLogs go to https://classic.warcraftlogs.com/api/clients and press the "Create Client" button.

After you have done this you will see this:
![App Image](https://github.com/ForsakenNGS/LogTrackerApp/blob/master/doc_resources/images/warcraftlogs/warcraftlogs_api_2.png?raw=true)
1. "Enter a name for your application:"
    + Enter any name you want. (It is recommended not to have LogTracker in the name)
2. "Enter one or more redirect URLs, separated by commas:"
    + Enter any valid URL of any Website - does not matter what you enter here.
3. "Public Client..."
    + Keep unchecked

**Save your API credentials as they will not be shown again after you leave the page.**

If you want to check your remaining points manually from the WarcraftLogs homepage, you can do so by going to https://classic.warcraftlogs.com/profile and scrolling to the bottom.
![App Image](https://github.com/ForsakenNGS/LogTrackerApp/blob/master/doc_resources/images/warcraftlogs/warcraftlogs_api_1.png?raw=true)
If it does not show up you need to click the "Set" button. Name can be left blank (and does not have to match the name you entered earlier)
