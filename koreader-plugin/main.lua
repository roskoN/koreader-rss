local DataStorage = require("datastorage")
local Device = require("device")
local InfoMessage = require("ui/widget/infomessage")
local InputDialog = require("ui/widget/inputdialog")
local ConfirmBox = require("ui/widget/confirmbox")
local Menu = require("ui/widget/menu")
local NetworkMgr = require("ui/network/manager")
local ReaderUI = require("apps/reader/readerui")
local Trapper = require("ui/trapper")
local UIManager = require("ui/uimanager")
local WidgetContainer = require("ui/widget/container/widgetcontainer")
local util = require("util")
local _ = require("gettext")

local RSSReader = WidgetContainer:extend{
    name = "rssreader",
    is_doc_only = false,
}

local function show(text)
    UIManager:show(InfoMessage:new{ text = text })
end

local function trim(text)
    return (text or ""):match("^%s*(.-)%s*$")
end

function RSSReader:init()
    local root = DataStorage:getDataDir()
    self.backend = self.path .. "/bin/rss-backend"
    self.data_dir = root .. "/data/rssreader"
    self.cache_dir = root .. "/cache/rssreader"
    self.database = self.data_dir .. "/rss.sqlite3"
    self.probe_database = self.data_dir .. "/probe.sqlite3"
    self.fixture = self.cache_dir .. "/integration-probe.html"
    self.ui.menu:registerToMainMenu(self)
end

function RSSReader:showUnread()
    local connection
    local statement
    local ok, result = pcall(function()
        local SQ3 = require("lua-ljsqlite3/init")
        connection = SQ3.open(self.database)
        statement = connection:prepare([[
            SELECT a.id, a.title, f.title, a.sort_at, a.url
            FROM articles a JOIN feeds f ON f.id = a.feed_id
            WHERE a.is_read = 0
            ORDER BY a.sort_at DESC, a.id DESC
            LIMIT 100
        ]])
        local items = {}
        while true do
            local row = statement:step()
            if not row then break end
            local published = tonumber(row[4])
            table.insert(items, {
                article_id = tonumber(row[1]),
                text = tostring(row[2]),
                mandatory = string.format(
                    "%s · %s",
                    tostring(row[3] or _("Unknown feed")),
                    published and os.date("%Y-%m-%d", published) or _("Unknown date")
                ),
                url = row[5],
            })
        end
        return items
    end)
    if statement then pcall(statement.close, statement) end
    if connection then pcall(connection.close, connection) end
    if not ok then
        show(_("Cannot read unread articles:") .. "\n" .. tostring(result))
        return
    end
    if #result == 0 then
        show(_("No unread articles."))
        return
    end

    local menu
    menu = Menu:new{
        title = string.format(_("Unread articles (%d)"), #result),
        item_table = result,
        covers_fullscreen = true,
        is_borderless = true,
        is_popout = false,
        title_bar_fm_style = true,
        onMenuSelect = function(_, item)
            self:openArticle(item.article_id)
        end,
        onMenuHold = function(_, item)
            if item.url and item.url ~= "" then Device:openLink(item.url) end
        end,
    }
    UIManager:show(menu)
end

function RSSReader:markArticle(article_id, state)
    self:runBackend({ self.backend, "--db", self.database, "mark", tostring(article_id), state },
        _("Updating article…"), function() self:showUnread() end)
end

function RSSReader:showAllArticles(feed_id, title, offset)
    offset = offset or 0
    local connection
    local statement
    local ok, result = pcall(function()
        local SQ3 = require("lua-ljsqlite3/init")
        connection = SQ3.open(self.database)
        local sql = "SELECT a.id,a.title,COALESCE(f.title,f.source_url),a.sort_at,a.url FROM articles a JOIN feeds f ON f.id=a.feed_id"
        if feed_id then sql = sql .. " WHERE a.feed_id = " .. tostring(feed_id) end
        sql = sql .. " ORDER BY a.sort_at DESC,a.id DESC LIMIT 100 OFFSET " .. tostring(offset)
        statement = connection:prepare(sql)
        local items = {}
        while true do
            local row = statement:step()
            if not row then break end
            table.insert(items, {
                article_id = tonumber(row[1]),
                text = tostring(row[2]),
                mandatory = string.format("%s · %s", tostring(row[3]), os.date("%Y-%m-%d", tonumber(row[4]))),
                url = row[5],
            })
        end
        if #items == 100 then
            table.insert(items, { text = _("Next page →"), next_offset = offset + 100 })
        end
        return items
    end)
    if statement then pcall(statement.close, statement) end
    if connection then pcall(connection.close, connection) end
    if not ok then show(tostring(result)); return end
    if #result == 0 then show(_("No articles.")); return end
    local menu = Menu:new{
        title = title or _("All articles"), item_table = result, covers_fullscreen = true,
        onMenuSelect = function(_, item)
            if item.next_offset then
                UIManager:close(menu)
                self:showAllArticles(feed_id, title, item.next_offset)
            else
                self:openArticle(item.article_id)
            end
        end,
        onMenuHold = function(_, item)
            if item.url and item.url ~= "" then Device:openLink(item.url) end
        end,
    }
    UIManager:show(menu)
end

function RSSReader:showStatus()
    self:runBackend({ self.backend, "--db", self.database, "status" }, _("Loading refresh status…"), show)
end

function RSSReader:refreshNow()
    self:runBackend({ self.backend, "--db", self.database, "refresh", "--reason", "manual" },
        _("Refreshing feeds…"), function() self:showStatus() end)
end

function RSSReader:showFeeds()
    self:ensureDirectories()
    self:runBackend({ self.backend, "--db", self.database, "feed", "list" },
        _("Loading feeds…"), function(output)
            local items = {}
            for line in (output .. "\n"):gmatch("([^\n]*)\n") do
                local id, enabled, title, url = line:match("^(%d+)\t([^\t]*)\t([^\t]*)\t(.+)$")
                if id then
                    local is_enabled = enabled == "enabled"
                    local label = (title ~= "" and title or url)
                    table.insert(items, { feed_id = tonumber(id), enabled = is_enabled, text = (is_enabled and "[on] " or "[off] ") .. label, mandatory = url })
                end
            end
            if #items == 0 then show(_("No feeds configured.")); return end
            local menu
            menu = Menu:new{
                title = _("Feeds"), item_table = items, covers_fullscreen = true,
                onMenuSelect = function(_, item) self:showFeedActions(item, menu) end,
                onMenuHold = function(_, item) self:toggleFeed(item, menu) end,
            }
            UIManager:show(menu)
        end)
end

function RSSReader:showFeedActions(item, feeds_menu)
    local actions = Menu:new{
        title = item.text,
        item_table = {
            { text = _("Articles"), callback = function()
                UIManager:close(actions); UIManager:close(feeds_menu)
                self:showAllArticles(item.feed_id, item.text)
            end },
            { text = item.enabled and _("Disable feed") or _("Enable feed"), callback = function()
                UIManager:close(actions); self:toggleFeed(item, feeds_menu)
            end },
            { text = _("Remove feed"), callback = function()
                UIManager:close(actions); self:confirmRemoveFeed(item.feed_id, item.text, feeds_menu)
            end },
        },
        covers_fullscreen = true,
    }
    UIManager:show(actions)
end

function RSSReader:toggleFeed(item, menu)
    local command = item.enabled and "disable" or "enable"
    self:runBackend({ self.backend, "--db", self.database, "feed", command, tostring(item.feed_id) },
        item.enabled and _("Disabling feed…") or _("Enabling feed…"), function()
            UIManager:close(menu)
            self:showFeeds()
        end)
end

function RSSReader:confirmRemoveFeed(feed_id, title, menu)
    UIManager:show(ConfirmBox:new{
        text = string.format(_("Remove feed '%s'?"), title),
        ok_text = _("Remove"),
        ok_callback = function()
            self:runBackend({ self.backend, "--db", self.database, "feed", "remove", tostring(feed_id) },
                _("Removing feed…"), function() UIManager:close(menu); self:showFeeds() end)
        end,
    })
end

function RSSReader:addFeedDialog()
    local dialog
    local function submit()
        local url = trim(dialog:getInputText())
        if url == "" or url == "https://" then
            show(_("Enter a feed URL."))
            return
        end
        UIManager:close(dialog)
        self:runBackend({ self.backend, "--db", self.database, "feed", "add", url },
            _("Adding feed…"), function() show(_("Feed added.")) end)
    end
    dialog = InputDialog:new{
        title = _("Add feed URL"), input = "https://",
        buttons = {{
            {
                text = _("Cancel"), id = "close",
                callback = function() UIManager:close(dialog) end,
            },
            {
                text = _("Add"), is_enter_default = true,
                callback = submit,
            },
        }},
    }
    UIManager:show(dialog)
end

function RSSReader:openArticle(article_id)
    local ok, err = self:ensureDirectories()
    if not ok then show(tostring(err)); return end
    self:runBackend({ self.backend, "--db", self.database, "materialize", tostring(article_id), "--cache", self.cache_dir },
        _("Opening article…"), function(output)
            local path = trim(output):match("([^\r\n]+)$")
            if not path or path == "" then show(_("Backend returned no article path.")); return end
            ReaderUI:showReader(path, nil, nil, nil, function()
                self:runBackend({ self.backend, "--db", self.database, "mark", tostring(article_id), "read" },
                    _("Marking article read…"), function() end)
            end)
        end)
end

function RSSReader:ensureDirectories()
    local ok, err = util.makePath(self.data_dir)
    if not ok then return nil, err end
    ok, err = util.makePath(self.cache_dir)
    if not ok then return nil, err end
    return true
end

function RSSReader:runBackend(arguments, title, callback)
    local command = util.shell_escape(arguments)
        .. " 2>&1; status=$?; printf '\\n__RSS_EXIT=%s\\n' \"$status\""
    Trapper:wrap(function()
        local completed, output = Trapper:dismissablePopen(command, title)
        if not completed then
            show(_("RSS backend operation dismissed; the bounded process may finish in the background."))
            return
        end
        local status = output and output:match("__RSS_EXIT=(%d+)%s*$")
        local body = trim((output or ""):gsub("%s*__RSS_EXIT=%d+%s*$", ""))
        if status ~= "0" then
            show(_("RSS backend failed:") .. "\n" .. (body ~= "" and body or _("No diagnostic output")))
            return
        end
        callback(body)
    end)
end

function RSSReader:showEnvironment()
    local Screen = Device.screen
    show(string.format(
        "Screen: %d × %d\nData: %s\nCache: %s\nPlugin: %s\nBackend: %s",
        Screen:getWidth(), Screen:getHeight(), DataStorage:getDataDir(),
        self.cache_dir, self.path, self.backend
    ))
end

function RSSReader:runDoctor()
    self:runBackend({ self.backend, "doctor" }, _("Running RSS backend doctor…"), function(output)
        show(output)
    end)
end

function RSSReader:runHttpsProbe()
    NetworkMgr:runWhenOnline(function()
        self:runBackend(
            { self.backend, "http-probe", "--url", "https://example.com/" },
            _("Testing HTTPS and certificate roots…"),
            function(output) show(output) end
        )
    end)
end

function RSSReader:queryProbeDatabase()
    local connection
    local ok, result = pcall(function()
        local SQ3 = require("lua-ljsqlite3/init")
        connection = SQ3.open(self.probe_database)
        local sqlite_version = connection:rowexec("SELECT sqlite_version()")
        local value = connection:rowexec("SELECT value FROM meta WHERE key='probe'")
        local journal = connection:rowexec("PRAGMA journal_mode")
        return string.format(
            "Lua SQLite: %s\nProbe value: %s\nJournal: %s\nDatabase: %s",
            tostring(sqlite_version), tostring(value), tostring(journal), self.probe_database
        )
    end)
    if connection then
        pcall(connection.close, connection)
    end
    if ok then
        show(result)
    else
        show(_("SQLite probe failed:") .. "\n" .. tostring(result))
    end
end

function RSSReader:initializeAndQueryDatabase()
    local ok, err = self:ensureDirectories()
    if not ok then
        show(_("Cannot create RSS data directory:") .. "\n" .. tostring(err))
        return
    end
    self:runBackend(
        { self.backend, "init-probe-db", "--db", self.probe_database },
        _("Creating SQLite integration probe…"),
        function() self:queryProbeDatabase() end
    )
end

function RSSReader:materializeAndOpenFixture()
    local ok, err = self:ensureDirectories()
    if not ok then
        show(_("Cannot create RSS cache directory:") .. "\n" .. tostring(err))
        return
    end
    self:runBackend(
        { self.backend, "materialize-fixture", "--out", self.fixture },
        _("Materializing offline HTML fixture…"),
        function(output)
            local path = output:match("([^\r\n]+)%s*$")
            if not path or path == "" then
                show(_("Backend returned no fixture path."))
                return
            end
            ReaderUI:showReader(path, nil, nil, nil, function()
                show(_("Offline HTML fixture opened successfully."))
            end)
        end
    )
end

function RSSReader:addToMainMenu(menu_items)
    menu_items.rss_reader_probe = {
        text = _("News integration probe"),
        sub_item_table = {
            {
                text = _("Unread"),
                callback = function() self:showUnread() end,
            },
            {
                text = _("All articles"),
                callback = function() self:showAllArticles() end,
            },
            {
                text = _("Refresh now"),
                callback = function() self:refreshNow() end,
            },
            {
                text = _("Refresh status"),
                callback = function() self:showStatus() end,
            },
            {
                text = _("Feeds"),
                callback = function() self:showFeeds() end,
            },
            {
                text = _("Add feed"),
                callback = function() self:addFeedDialog() end,
            },
            {
                text = _("Environment and paths"),
                callback = function() self:showEnvironment() end,
            },
            {
                text = _("Run backend doctor"),
                callback = function() self:runDoctor() end,
            },
            {
                text = _("Test HTTPS and certificates"),
                callback = function() self:runHttpsProbe() end,
            },
            {
                text = _("Initialize and query SQLite"),
                callback = function() self:initializeAndQueryDatabase() end,
            },
            {
                text = _("Open offline HTML fixture"),
                callback = function() self:materializeAndOpenFixture() end,
            },
        },
    }
end

return RSSReader
