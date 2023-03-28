![Banner](https://github.com/ForsakenNGS/LogTrackerApp/blob/master/doc_resources/images/banner/LogTrackerBanner.png?raw=true)
![GitHub all releases](https://img.shields.io/github/downloads/ForsakenNGS/LogTrackerApp/total?label=Downloads) ![GitHub issues](https://img.shields.io/github/issues-raw/ForsakenNGS/LogTrackerApp?label=Open%20Issues) ![GitHub](https://img.shields.io/github/license/ForsakenNGS/LogTrackerApp?label=License)

# Disclaimer! This appliction is not officially endorsed by WarcraftLogs and the developer takes no responsibility for actions taken against your WarcraftLogs account.
No actions have yet been reported but this applciation is in a grey area. Meaning it is technically within their guidlines but those can change or be interpreted differently.

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
